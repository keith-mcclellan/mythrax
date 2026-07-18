// Sprint 4 Integration Test Suite: Parasitic Cognitive Callbacks

use std::sync::Arc;
use tempfile::tempdir;
use chrono::{Utc, Duration};
use serde_json::json;

use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::api::ApiState;
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::WatchIgnoreList;
use mythrax_core::db::cognitive_tasks::{CognitiveTask, TaskStatus};
use mythrax_core::mcp_routes::manage_handlers::handle_pre_invocation_hook;
use mythrax_core::mcp_routes::write_handlers::handle_cognitive_callback;

fn setup_env_vars() {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
}

async fn create_test_state(temp_dir: &tempfile::TempDir) -> anyhow::Result<ApiState> {
    let db_path = temp_dir.path().join("db");
    let backend = SurrealBackend::new(&formatsurreal_path(&db_path)).await?;
    backend.init().await?;

    let store = Arc::new(MarkdownStore::new(temp_dir.path())?);
    let ignore_list = Arc::new(WatchIgnoreList::new());

    Ok(ApiState {
        backend: Arc::new(backend),
        auth_token: "test".to_string(),
        store,
        ignore_list,
        dream_tx: None,
        shutdown_tx: None,
    })
}

fn formatsurreal_path(path: &std::path::Path) -> String {
    format!("surrealkv://{}", path.to_string_lossy())
}

#[tokio::test]
async fn test_cognitive_task_crud() -> anyhow::Result<()> {
    setup_env_vars();
    let temp_dir = tempdir()?;
    let state = create_test_state(&temp_dir).await?;
    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>().unwrap();

    let task_id = "cognitive_task:task123";
    let task = CognitiveTask {
        id: task_id.to_string(),
        task_type: "Synthesis".to_string(),
        prompt: "Synthesize the codebase structure".to_string(),
        system_instruction: "You are a senior developer".to_string(),
        expected_format: "Json".to_string(),
        priority: "Normal".to_string(),
        created_at: Utc::now(),
        status: "Pending".to_string(),
        result: None,
        ttl_minutes: 30,
        injected_at: None,
    };

    // 1. Create
    let created_id = surreal_backend.create_cognitive_task(&task).await?;
    assert_eq!(created_id, task_id);

    // 2. Read
    let retrieved_opt = surreal_backend.get_cognitive_task(task_id).await?;
    assert!(retrieved_opt.is_some());
    let retrieved = retrieved_opt.unwrap();
    assert_eq!(retrieved.prompt, task.prompt);
    assert_eq!(retrieved.status, "Pending");

    // 3. Get pending
    let pending = surreal_backend.get_pending_cognitive_tasks().await?;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, task_id);

    // 4. Update Status to Injected
    surreal_backend.update_cognitive_task_status(task_id, TaskStatus::Injected, None).await?;
    let retrieved = surreal_backend.get_cognitive_task(task_id).await?.unwrap();
    assert_eq!(retrieved.status, "Injected");
    assert!(retrieved.injected_at.is_some());

    // 5. Update Status to Completed with Result
    surreal_backend.update_cognitive_task_status(task_id, TaskStatus::Completed, Some("{\"key\": \"val\"}".to_string())).await?;
    let retrieved = surreal_backend.get_cognitive_task(task_id).await?.unwrap();
    assert_eq!(retrieved.status, "Completed");
    assert_eq!(retrieved.result, Some("{\"key\": \"val\"}".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_cognitive_task_injection() -> anyhow::Result<()> {
    setup_env_vars();
    let temp_dir = tempdir()?;
    let state = create_test_state(&temp_dir).await?;
    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>().unwrap();

    // Create 1 Immediate and 2 Normal tasks
    let task_imm = CognitiveTask {
        id: "cognitive_task:imm_task".to_string(),
        task_type: "Synthesis".to_string(),
        prompt: "Immediate Prompt Here".to_string(),
        system_instruction: "System instructions imm".to_string(),
        expected_format: "Any".to_string(),
        priority: "Immediate".to_string(),
        created_at: Utc::now(),
        status: "Pending".to_string(),
        result: None,
        ttl_minutes: 30,
        injected_at: None,
    };
    let task_norm1 = CognitiveTask {
        id: "cognitive_task:norm_task1".to_string(),
        task_type: "Compaction".to_string(),
        prompt: "Normal Prompt 1".to_string(),
        system_instruction: "System instructions norm 1".to_string(),
        expected_format: "Any".to_string(),
        priority: "Normal".to_string(),
        created_at: Utc::now() + Duration::seconds(1),
        status: "Pending".to_string(),
        result: None,
        ttl_minutes: 30,
        injected_at: None,
    };
    let task_norm2 = CognitiveTask {
        id: "cognitive_task:norm_task2".to_string(),
        task_type: "Extraction".to_string(),
        prompt: "Normal Prompt 2".to_string(),
        system_instruction: "System instructions norm 2".to_string(),
        expected_format: "Any".to_string(),
        priority: "Normal".to_string(),
        created_at: Utc::now() + Duration::seconds(2),
        status: "Pending".to_string(),
        result: None,
        ttl_minutes: 30,
        injected_at: None,
    };

    surreal_backend.create_cognitive_task(&task_imm).await?;
    surreal_backend.create_cognitive_task(&task_norm1).await?;
    surreal_backend.create_cognitive_task(&task_norm2).await?;

    // Pre-invocation call - should inject ONLY the Immediate task
    let payload = json!({
        "session_id": "test_session",
        "query": "Hello",
        "workspace_path": temp_dir.path().to_string_lossy()
    });

    let res = handle_pre_invocation_hook(&state, payload).await?;
    let text = res["content"][0]["text"].as_str().unwrap();

    assert!(text.contains("Immediate Prompt Here"));
    assert!(!text.contains("Normal Prompt 1"));
    assert!(!text.contains("Normal Prompt 2"));

    // Verify status
    let t_imm = surreal_backend.get_cognitive_task("cognitive_task:imm_task").await?.unwrap();
    assert_eq!(t_imm.status, "Injected");
    assert!(t_imm.injected_at.is_some());

    let t_norm1 = surreal_backend.get_cognitive_task("cognitive_task:norm_task1").await?.unwrap();
    assert_eq!(t_norm1.status, "Pending");

    // Complete the Immediate task
    surreal_backend.update_cognitive_task_status("cognitive_task:imm_task", TaskStatus::Completed, Some("done".to_string())).await?;

    // Pre-invocation again - should inject the 2 Normal tasks
    let payload2 = json!({
        "session_id": "test_session",
        "query": "Hello again",
        "workspace_path": temp_dir.path().to_string_lossy()
    });

    let res2 = handle_pre_invocation_hook(&state, payload2).await?;
    let text2 = res2["content"][0]["text"].as_str().unwrap();

    assert!(!text2.contains("Immediate Prompt Here"));
    assert!(text2.contains("Normal Prompt 1"));
    assert!(text2.contains("Normal Prompt 2"));

    // Verify status of normal tasks is now Injected
    let t_norm1 = surreal_backend.get_cognitive_task("cognitive_task:norm_task1").await?.unwrap();
    assert_eq!(t_norm1.status, "Injected");
    let t_norm2 = surreal_backend.get_cognitive_task("cognitive_task:norm_task2").await?.unwrap();
    assert_eq!(t_norm2.status, "Injected");

    Ok(())
}

#[tokio::test]
async fn test_cognitive_callback_validation() -> anyhow::Result<()> {
    setup_env_vars();
    let temp_dir = tempdir()?;
    let state = create_test_state(&temp_dir).await?;
    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>().unwrap();

    // 1. Task with Json format
    let task = CognitiveTask {
        id: "cognitive_task:task_json".to_string(),
        task_type: "Refinement".to_string(),
        prompt: "Prompt".to_string(),
        system_instruction: "Sys".to_string(),
        expected_format: "Json".to_string(),
        priority: "Normal".to_string(),
        created_at: Utc::now(),
        status: "Pending".to_string(),
        result: None,
        ttl_minutes: 30,
        injected_at: None,
    };
    surreal_backend.create_cognitive_task(&task).await?;

    // Call callback on Pending task -> should fail
    let callback_payload = json!({
        "callback_id": "cognitive_task:task_json",
        "result": "{\"valid\": true}"
    });
    let callback_res = handle_cognitive_callback(&state, callback_payload.clone()).await;
    assert!(callback_res.is_err(), "Callback on Pending status must fail");

    // Move status to Injected
    surreal_backend.update_cognitive_task_status("cognitive_task:task_json", TaskStatus::Injected, None).await?;

    // Call callback with invalid JSON format -> should fail
    let bad_json_payload = json!({
        "callback_id": "cognitive_task:task_json",
        "result": "{bad json"
    });
    let callback_res = handle_cognitive_callback(&state, bad_json_payload).await;
    assert!(callback_res.is_err(), "Callback with malformed JSON must fail");

    // Call callback with valid JSON format -> should succeed
    let good_payload = json!({
        "callback_id": "cognitive_task:task_json",
        "result": "{\"valid\": true}"
    });
    let callback_res = handle_cognitive_callback(&state, good_payload).await?;
    assert_eq!(callback_res["status"], "success");

    let final_task = surreal_backend.get_cognitive_task("cognitive_task:task_json").await?.unwrap();
    assert_eq!(final_task.status, "Completed");
    assert_eq!(final_task.result, Some("{\"valid\": true}".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_cognitive_fallback_disabled() -> anyhow::Result<()> {
    setup_env_vars();
    let _temp_dir = tempdir()?;
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    let llm = mythrax_core::llm::LLMClient::new();
    let profile = mythrax_core::contracts::TaskProfile::new(mythrax_core::contracts::TaskArchetype::Reasoning);

    unsafe {
        std::env::set_var("MYTHRAX_DISABLE_FALLBACK", "true");
        std::env::set_var("MYTHRAX_TEST_TIMEOUT_SECS", "0");
        std::env::remove_var("MYTHRAX_TEST_MOCK");
    }

    let res = llm.routed_completion(&backend, &profile, None, "test prompt").await;

    unsafe {
        std::env::remove_var("MYTHRAX_DISABLE_FALLBACK");
        std::env::remove_var("MYTHRAX_TEST_TIMEOUT_SECS");
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
    }

    assert!(res.is_err(), "Completion must fail when fallback is disabled");
    let err_msg = res.unwrap_err().to_string();
    assert!(
        err_msg.contains("Cognitive callback for cloud model timed out and fallbacks are disabled"),
        "Unexpected error: {}", err_msg
    );

    Ok(())
}

#[tokio::test]
async fn test_pipeline_state_serialization() -> anyhow::Result<()> {
    setup_env_vars();
    let temp_dir = tempdir()?;
    let state = create_test_state(&temp_dir).await?;
    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>().unwrap();

    let target_file = temp_dir.path().join("out.txt");
    let callback_id = "cognitive_task:cb_pipeline";
    
    // Save pipeline state
    let state_json = json!({
        "target_file": target_file.to_string_lossy().to_string(),
        "extra_info": "sprint4"
    }).to_string();

    surreal_backend.save_pipeline_state(callback_id, &state_json).await?;

    // Assert it exists
    let saved = surreal_backend.get_pipeline_state(callback_id).await?;
    assert!(saved.is_some());
    assert_eq!(saved.unwrap(), state_json);

    // Create the task in Injected status
    let task = CognitiveTask {
        id: callback_id.to_string(),
        task_type: "Refinement".to_string(),
        prompt: "Prompt".to_string(),
        system_instruction: "Sys".to_string(),
        expected_format: "Any".to_string(),
        priority: "Normal".to_string(),
        created_at: Utc::now(),
        status: "Injected".to_string(),
        result: None,
        ttl_minutes: 30,
        injected_at: Some(Utc::now()),
    };
    surreal_backend.create_cognitive_task(&task).await?;

    // Trigger the callback
    let callback_payload = json!({
        "callback_id": callback_id,
        "result": "Hello Continuation!"
    });
    handle_cognitive_callback(&state, callback_payload).await?;

    // Verify continuation executed downstream steps (atomic file write/rename)
    assert!(target_file.exists());
    let file_content = std::fs::read_to_string(&target_file)?;
    assert_eq!(file_content, "Hello Continuation!");

    // Verify pipeline state is deleted
    let post_saved = surreal_backend.get_pipeline_state(callback_id).await?;
    assert!(post_saved.is_none());

    Ok(())
}

#[tokio::test]
async fn test_ttl_sweep_fallback() -> anyhow::Result<()> {
    setup_env_vars();
    let temp_dir = tempdir()?;
    let state = create_test_state(&temp_dir).await?;
    let surreal_backend = state.backend.as_any().downcast_ref::<SurrealBackend>().unwrap();

    let target_file = temp_dir.path().join("out_ttl.txt");
    let callback_id = "cognitive_task:cb_ttl";

    // 1. Create a task with negative TTL (already expired) and Injected status
    let task = CognitiveTask {
        id: callback_id.to_string(),
        task_type: "Refinement".to_string(),
        prompt: "TTL Prompt".to_string(),
        system_instruction: "TTL Sys".to_string(),
        expected_format: "Any".to_string(),
        priority: "Normal".to_string(),
        created_at: Utc::now() - Duration::minutes(40),
        status: "Injected".to_string(),
        result: None,
        ttl_minutes: 10,
        injected_at: Some(Utc::now() - Duration::minutes(20)),
    };
    surreal_backend.create_cognitive_task(&task).await?;

    // Save pipeline state for continuation
    let state_json = json!({
        "target_file": target_file.to_string_lossy().to_string()
    }).to_string();
    surreal_backend.save_pipeline_state(callback_id, &state_json).await?;

    // 2. Call pre-invocation hook, which triggers the TTL Sweep
    let payload = json!({
        "session_id": "test_session_ttl",
        "query": "Hello",
        "workspace_path": temp_dir.path().to_string_lossy()
    });

    handle_pre_invocation_hook(&state, payload).await?;

    // 3. Verify task is Expired and has fallback result
    let updated_task = surreal_backend.get_cognitive_task(callback_id).await?.unwrap();
    assert_eq!(updated_task.status, "Expired");
    assert!(updated_task.result.is_some());
    let fallback_result = updated_task.result.unwrap();

    // Verify continuation ran on fallback
    assert!(target_file.exists());
    let file_content = std::fs::read_to_string(&target_file)?;
    assert_eq!(file_content, fallback_result);

    // Save pipeline state again to verify late cloud callback can overwrite
    surreal_backend.save_pipeline_state(callback_id, &state_json).await?;

    // 4. Late cloud callback arrives with cloud result -> should succeed and overwrite
    let late_cloud_payload = json!({
        "callback_id": callback_id,
        "result": "Cloud Wins!"
    });
    handle_cognitive_callback(&state, late_cloud_payload).await?;

    let final_task = surreal_backend.get_cognitive_task(callback_id).await?.unwrap();
    assert_eq!(final_task.status, "Completed");
    assert_eq!(final_task.result, Some("Cloud Wins!".to_string()));

    // Verify continuation ran again on late cloud result
    let file_content_final = std::fs::read_to_string(&target_file)?;
    assert_eq!(file_content_final, "Cloud Wins!");

    Ok(())
}
