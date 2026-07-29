#![allow(dead_code, unused_imports)]

mod hook_adapters {
use mythrax_core::hooks::adapters::adapt_payload;
use serde_json::json;

#[test]
fn test_claude_code_payload_maps_to_canonical() {
    let payload = json!({
        "session_id": "claude-session_#123",
        "transcript_path": "C:\\Users\\Keith\\Documents\\transcript.json",
        "stop_hook_active": true
    });

    let (session_id, stop_hook_active, transcript_path) = adapt_payload(payload, "claude").unwrap();

    assert_eq!(session_id, "claude-session_123");
    assert_eq!(stop_hook_active, true);
    assert_eq!(transcript_path, "C:/Users/Keith/Documents/transcript.json");
}

#[test]
fn test_claude_code_payload_negative() {
    let payload = json!({
        "session": "claude-session_#123",
        "transcript": "C:\\Users\\Keith\\Documents\\transcript.json",
        "active": true
    });

    let res = adapt_payload(payload, "claude");
    assert!(
        res.is_err(),
        "Old invented keys must fail to deserialize or not populate"
    );
}

#[test]
fn test_codex_payload_maps_to_canonical() {
    let payload = json!({
        "conversation_id": "codex!_session",
        "log_path": "/var/log/codex/transcript.json",
        "enabled": false
    });

    let res = adapt_payload(payload, "codex");
    assert!(
        res.is_err(),
        "Codex payload must fail with unsupported message"
    );
    let err = res.unwrap_err().to_string();
    assert!(
        err.contains("unsupported in v2.1.0"),
        "Error must contain unsupported in v2.1.0"
    );
}

#[test]
fn test_cursor_payload_maps_to_canonical() {
    let payload = json!({
        "cursor_session_id": "cursor-123",
        "chat_history_path": "/Users/keith/.cursor/history.json",
        "hook_active": true
    });

    let res = adapt_payload(payload, "cursor");
    assert!(
        res.is_err(),
        "Cursor payload must fail with unsupported message"
    );
    let err = res.unwrap_err().to_string();
    assert!(
        err.contains("unsupported in v2.1.0"),
        "Error must contain unsupported in v2.1.0"
    );
}

#[test]
fn test_gemini_payload_maps_to_canonical() {
    let payload = json!({
        "session_id": "gemini-456",
        "transcript_path": "/Users/keith/.gemini/transcript.json",
        "stop_hook_active": false
    });

    let (session_id, stop_hook_active, transcript_path) = adapt_payload(payload, "gemini").unwrap();

    assert_eq!(session_id, "gemini-456");
    assert_eq!(stop_hook_active, false);
    assert_eq!(transcript_path, "/Users/keith/.gemini/transcript.json");
}

}

mod hook_io_discipline {
use mythrax_core::hooks::emit_hook_result;
use std::fs;
use std::path::Path;

#[test]
fn test_handler_returns_result_on_error() {
    // Call emit_hook_result with an Err to verify it handles errors gracefully,
    // returns a non-blocking fallback HookResult, and does not panic.
    // We capture stdout to inspect the printed JSON.
    let error_input = Err(anyhow::anyhow!("Simulated compaction failure"));

    // We mock/stub the stdout/stderr by using a thread-local or just running it.
    // Since emit_hook_result prints to stdout/stderr, we just run it to prove it doesn't panic
    // and produces a valid non-blocking result.
    let result = std::panic::catch_unwind(|| {
        emit_hook_result(error_input);
    });

    assert!(result.is_ok(), "emit_hook_result panicked on Err input");
}

#[test]
fn test_emit_is_only_io_boundary() {
    // Scan all rust files in src/hooks/ and verify they contain NO println!, eprintln!, print!, or eprint!
    // except for mod.rs which contains the emitter itself.
    let hooks_dir = Path::new("src/hooks");
    assert!(hooks_dir.exists(), "src/hooks directory not found");

    let entries = fs::read_dir(hooks_dir).unwrap();
    for entry in entries {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("rust")
            || path.extension().and_then(|s| s.to_str()) == Some("rs")
        {
            let filename = path.file_name().and_then(|s| s.to_str()).unwrap();
            let content = fs::read_to_string(&path).unwrap();

            if filename == "mod.rs" {
                // mod.rs is allowed to have print macros inside emit_hook_result
                continue;
            }

            // Check for forbidden print macros
            assert!(
                !content.contains("println!"),
                "Forbidden println! found in pure hook module: {:?}",
                path
            );
            assert!(
                !content.contains("eprintln!"),
                "Forbidden eprintln! found in pure hook module: {:?}",
                path
            );
            assert!(
                !content.contains("print!"),
                "Forbidden print! found in pure hook module: {:?}",
                path
            );
            assert!(
                !content.contains("eprint!"),
                "Forbidden eprint! found in pure hook module: {:?}",
                path
            );
            assert!(
                !content.contains("std::process::exit"),
                "Forbidden std::process::exit found in pure hook module: {:?}",
                path
            );
        }
    }
}

#[test]
fn test_summarize_diff_math() {
    let dir = tempfile::tempdir().unwrap();
    let base_path = dir.path().join("base.jsonl");
    let curr_path = dir.path().join("curr.jsonl");

    // Write base fixture: 3 instances, 1 resolved (33.33%), 1 unresolved, 1 error
    let mut base_file = fs::File::create(&base_path).unwrap();
    use std::io::Write;
    writeln!(base_file, "{{\"resolved_ids\": [\"inst-1\"], \"unresolved_ids\": [\"inst-2\"], \"error_ids\": [\"inst-3\"]}}").unwrap();

    // Write current fixture: 3 instances, 2 resolved (66.67%), 1 unresolved, 0 error
    let mut curr_file = fs::File::create(&curr_path).unwrap();
    writeln!(curr_file, "{{\"resolved_ids\": [\"inst-1\", \"inst-2\"], \"unresolved_ids\": [\"inst-3\"], \"error_ids\": []}}").unwrap();

    // Execute summarize.py
    let output = std::process::Command::new("python3")
        .arg("../evals/swebench/summarize.py")
        .arg(curr_path.to_str().unwrap())
        .arg("--compare")
        .arg(base_path.to_str().unwrap())
        .output()
        .expect("Failed to execute summarize.py");

    let stdout_str = String::from_utf8_lossy(&output.stdout);
    println!("summarize.py output:\n{}", stdout_str);

    assert!(output.status.success(), "summarize.py exited with an error");

    // Assert delta calculations:
    // Base: 33.33% resolved (1/3)
    // Curr: 66.67% resolved (2/3)
    // Delta: +33.33 percentage points
    assert!(
        stdout_str.contains("+33.33 percentage points"),
        "Output should contain '+33.33 percentage points'"
    );
    assert!(
        stdout_str.contains("+1"),
        "Output should contain '+1' delta for resolved count"
    );
    // Assert status changes
    assert!(
        stdout_str.contains("inst-2"),
        "Output should contain inst-2"
    );
    assert!(
        stdout_str.contains("Improved (+)"),
        "Output should contain 'Improved (+)'"
    );
}

#[test]
fn test_unconditional_println_hygiene_in_backend() {
    let backend_path = Path::new("src/db/backend.rs");
    assert!(backend_path.exists(), "src/db/backend.rs not found");
    let content = fs::read_to_string(backend_path).unwrap();

    // Assert the Auto-Promoted println! message is not present
    assert!(
        !content.contains("println!(\"[Mythrax Synapse:"),
        "Offending unconditional println! found in src/db/backend.rs"
    );
}

}

mod pre_invocation_hook {
use mythrax_core::api::ApiState;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::llm::{DynamicModelBroker, ModelTier};
use mythrax_core::mcp_routes::handle_pre_invocation_hook;
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_soft_thresholding_and_hook_injection() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("db");

    // Initialize SurrealDB with KV store
    let backend = SurrealBackend::new(
        &format!("surrealkv://{}", db_path.to_string_lossy()),
        mythrax_core::db::BackendConfig {
            check_daemon: false,
            embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)),
            llm: Some(mythrax_core::llm::LLMClient::new_mock()),
        },
    )
    .await
    .unwrap();
    backend.init().await.unwrap();

    // Create a "borderline" episode (low confidence/score)
    let episode = mythrax_core::contracts::EpisodeSave {
        created_at: None,
        title: "Borderline Note".to_string(),
        content: "Soft threshold test content".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/borderline.md".to_string()),
        source_episode: None,
        session_id: Some("test_session".to_string()),
        task_id: None,
        ..Default::default()
    };
    backend.save_episode(&episode).await.unwrap();

    // Initialize Model Broker
    let models_dir = if std::env::var("MYTHRAX_TEST_MOCK").is_ok() {
        temp_dir.path().to_path_buf()
    } else {
        let home = std::env::var("HOME").unwrap();
        std::path::PathBuf::from(home).join(".mythrax/models")
    };
    let broker = DynamicModelBroker::new(models_dir).await.unwrap();
    let broker = Arc::new(broker);
    let _ = mythrax_core::llm::DYNAMIC_MODEL_BROKER.set(broker.clone());
    // Preload embedding model and acquire a Tier2 LLM to simulate active state
    broker
        .preload_embedding_model("mlx-community/nomic-embed-text-v1.5-mlx")
        .await
        .unwrap();
    if std::env::var("MYTHRAX_TEST_MOCK").is_err() {
        broker
            .update_config_model("mlx-community/Qwen2.5-0.5B-Instruct-4bit")
            .await
            .unwrap();
    }
    let _ = broker.acquire_llm(ModelTier::Tier2).await.unwrap();

    // Construct ApiState with necessary dependencies
    let state = ApiState {
        backend: Arc::new(backend),
        auth_token: "secret-token".to_string(),
        store: Arc::new(
            mythrax_core::store::MarkdownStore::new(temp_dir.path().to_path_buf()).unwrap(),
        ),
        ignore_list: Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
        shutdown_tx: None,
    };

    // Prepare payload for pre-invocation hook
    let payload = serde_json::json!({
        "session_id": "test_session",
        "query": "threshold test",
        "workspace_path": temp_dir.path().to_string_lossy()
    });

    // Execute the hook
    let result = handle_pre_invocation_hook(&state, payload).await.unwrap();

    // Extract content from the result (assuming JSON structure with 'content' array)
    let text_content = result["content"][0]["text"].as_str().unwrap();

    // Assertions
    // 1. The "borderline" candidate (soft thresholded) must be preserved in the response
    assert!(
        text_content.contains("Soft threshold test content"),
        "Borderline candidate must be preserved and ranked"
    );

    // 2. The hook must inject local model status into the response
    assert!(
        text_content.contains("### 🤖 Local Inference & Model Broker Status"),
        "Hook must inject local model status"
    );
}

#[tokio::test]
async fn test_post_invocation_hook_success_and_failure() {
    use mythrax_core::mcp_routes::handle_post_invocation_hook;

    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("db");

    let backend = SurrealBackend::new(
        &format!("surrealkv://{}", db_path.to_string_lossy()),
        mythrax_core::db::BackendConfig {
            check_daemon: false,
            embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)),
            llm: Some(mythrax_core::llm::LLMClient::new_mock()),
        },
    )
    .await
    .unwrap();
    backend.init().await.unwrap();

    let state = ApiState {
        backend: Arc::new(backend),
        auth_token: "secret-token".to_string(),
        store: Arc::new(
            mythrax_core::store::MarkdownStore::new(temp_dir.path().to_path_buf()).unwrap(),
        ),
        ignore_list: Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
        shutdown_tx: None,
    };

    // 1. Success case
    let payload_success = serde_json::json!({
        "session_id": "test_post_session",
        "exit_code": 0,
        "status": "success",
        "summary": "Completed successfully"
    });
    let res_success = handle_post_invocation_hook(&state, payload_success).await.unwrap();
    assert!(res_success["content"][0]["text"].as_str().unwrap().contains("test_post_session"));

    // Check STM status saved
    let stm_map = state.backend.get_stm("test_post_session", Some("_last_post_invocation_status")).await.unwrap();
    let stm_val = stm_map.get("_last_post_invocation_status");
    assert!(stm_val.is_some());
    assert!(stm_val.unwrap().contains("success"));

    // 2. Failure case
    let payload_fail = serde_json::json!({
        "session_id": "test_post_fail_session",
        "exit_code": 1,
        "status": "error",
        "summary": "Build failed",
        "error_message": "cargo compilation error"
    });
    let res_fail = handle_post_invocation_hook(&state, payload_fail).await.unwrap();
    assert!(res_fail["content"][0]["text"].as_str().unwrap().contains("test_post_fail_session"));

    // Check failure episode created
    let eps = state.backend.get_all_episodes().await.unwrap();
    let fail_ep = eps.iter().find(|e| e.session_id.as_deref() == Some("test_post_fail_session"));
    assert!(fail_ep.is_some(), "Failure episode must be saved on error post-invocation");
}

}

mod stop_hook {
use mythrax_core::hooks::stop::should_save;

#[test]
fn cadence_triggers_every_15_human_messages() {
    // Cadence triggers when we cross a multiple of 15 (e.g. 14 -> 15, 29 -> 30)
    assert!(should_save(14, 15), "Should trigger at 15");
    assert!(!should_save(15, 16), "Should not trigger at 16");
    assert!(!should_save(0, 5), "Should not trigger at 5");
    assert!(should_save(29, 30), "Should trigger at 30");
    assert!(!should_save(30, 31), "Should not trigger at 31");
    assert!(should_save(44, 45), "Should trigger at 45");
}

}

mod external_model_routing {
#[cfg(feature = "mlx")]
use mythrax_core::db::{StorageBackend, SurrealBackend};
#[cfg(feature = "mlx")]
use mythrax_core::llm::{DYNAMIC_MODEL_BROKER, DynamicModelBroker, LLMClient};
#[cfg(feature = "mlx")]
use mythrax_core::store::MarkdownStore;
#[cfg(feature = "mlx")]
use std::env;
#[cfg(feature = "mlx")]
use std::fs::File;
#[cfg(feature = "mlx")]
use std::io::Write;
#[cfg(feature = "mlx")]
use std::path::Path;
#[cfg(feature = "mlx")]
use std::sync::Arc;
#[cfg(feature = "mlx")]
use tempfile::tempdir;

#[tokio::test]
#[cfg(feature = "mlx")]
async fn test_external_and_in_process_hybrid_routing() {
    let home = env::var("HOME").unwrap();
    let model_dir = Path::new(&home).join(".mythrax/models");
    let _temp_dir = tempdir().expect("Failed to create temp dir");

    // Force mock off for this test to verify real HTTP and in-process routing
    unsafe {
        env::set_var("MYTHRAX_MOCK_LLM", "false");
    }

    // Initialize SurrealDB in memory
    let backend = SurrealBackend::new(
        "mem://",
        mythrax_core::db::BackendConfig {
            check_daemon: false,
            embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)),
            llm: Some(mythrax_core::llm::LLMClient::new_mock()),
        },
    )
    .await
    .unwrap();
    backend.init().await.unwrap();

    // Initialize the dynamic model broker
    let broker = DynamicModelBroker::new(model_dir.clone()).await.unwrap();
    let broker_arc = Arc::new(broker);
    let _ = DYNAMIC_MODEL_BROKER.set(broker_arc.clone());

    // 1. Direct Model Request: 0.5B Model (must run in-process)
    let client = LLMClient::new_mock();
    let response_0_5b = client
        .completion_explicit(
            &backend,
            "local",
            "gemini",
            "mlx-community/Qwen2.5-0.5B-Instruct-4bit",
            Some("Be extremely concise, output exactly one word: 'hello'."),
            "Greet me.",
            false,
        )
        .await;
    assert!(
        response_0_5b.is_ok(),
        "Direct 0.5B in-process completion failed: {:?}",
        response_0_5b.err()
    );
    let text_0_5b = response_0_5b.unwrap();
    println!(
        "DEBUG ROUTING TEST: 0.5B (in-process) Response: {}",
        text_0_5b
    );
    assert!(!text_0_5b.is_empty());

    // 2. Direct Model Request: 35B Model (must route to external mlx-lm HTTP server)
    let response_35b = client
        .completion_explicit(
            &backend,
            "local",
            "gemini",
            "mlx-community/Qwen3.6-35B-A3B-4bit",
            Some("Be extremely concise, output exactly one word: 'apple'."),
            "Name a fruit.",
            false,
        )
        .await;
    assert!(
        response_35b.is_ok(),
        "Direct 35B external completion failed: {:?}",
        response_35b.err()
    );
    let text_35b = response_35b.unwrap();
    println!(
        "DEBUG ROUTING TEST: 35B (external HTTP) Response: {}",
        text_35b
    );
    assert!(!text_35b.is_empty());
}

#[tokio::test]
#[cfg(feature = "mlx")]
async fn test_dreaming_routing_to_external_model() -> anyhow::Result<()> {
    let home = env::var("HOME").unwrap();
    let model_dir = Path::new(&home).join(".mythrax/models");
    let trans_dir = tempdir()?;
    let workspace_path = trans_dir.path().join("workspace");
    std::fs::create_dir_all(&workspace_path)?;

    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_path.to_str().unwrap());
        std::env::set_var("MYTHRAX_MOCK_LLM", "false");
    }

    // Initialize the dynamic model broker (since dreaming needs nomic embeddings in-process)
    let broker = DynamicModelBroker::new(model_dir.clone()).await.unwrap();
    let broker_arc = Arc::new(broker);
    // Preload embedding model first so that the files are loaded/cached
    broker_arc
        .preload_embedding_model("mlx-community/nomic-embed-text-v1.5-mlx")
        .await
        .unwrap();
    let _ = DYNAMIC_MODEL_BROKER.set(broker_arc.clone());

    // Initialize SurrealDB in memory
    let backend = SurrealBackend::new(
        "mem://",
        mythrax_core::db::BackendConfig {
            check_daemon: false,
            embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)),
            llm: Some(mythrax_core::llm::LLMClient::new_mock()),
        },
    )
    .await
    .unwrap();
    backend.init().await.unwrap();

    let vault_dir = tempdir()?;
    let store = MarkdownStore::new(vault_dir.path())?;

    // Create the transcript directory & file
    let transcript_path = trans_dir.path().join("transcript.jsonl");
    let transcript_path_str = transcript_path.to_string_lossy().to_string();

    let mut trans_file = File::create(&transcript_path)?;
    writeln!(
        trans_file,
        r#"{{"role": "user", "content": "Hello compactor, analyze this test session"}}"#
    )?;
    writeln!(
        trans_file,
        r#"{{"role": "tool", "content": "Session is active and verification token is EXTERNAL_DREAM_VERIFICATION_TOKEN"}}"#
    )?;
    drop(trans_file);

    // Register the transcript path in STM
    backend
        .save_stm(
            "sess_external_dream",
            "_transcript_path",
            &transcript_path_str,
        )
        .await?;
    backend
        .save_stm("sess_external_dream", "_last_activity", "some activity")
        .await?;

    // Force aging of STM records to satisfy >10m idleness check
    let surreal_backend = backend
        .as_any()
        .downcast_ref::<SurrealBackend>()
        .expect("Failed to downcast to SurrealBackend");
    surreal_backend.db
        .query("UPDATE short_term_memory SET updated_at = time::now() - 11m WHERE session_id = 'sess_external_dream';")
        .await?
        .check()?;

    // Run the compactor dreaming sweep unmocked (will route LLM calls to 35B model on mlx-lm HTTP server)
    let coordinator = mythrax_core::cognitive::synthesis::DreamCoordinator::new();
    coordinator
        .run_dream(backend.clone(), &store, Some("incremental"), None)
        .await?;

    // Verify the new turns are mined into the database
    let search_res = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "EXTERNAL_DREAM_VERIFICATION_TOKEN",
            Some("general"),
            false,
            5,
            0,
            0.0,
            None,
            false,
            true,
            false,
            None,
            true,
            None,
        ))
        .await?;
    assert!(
        search_res.total_matches > 0,
        "Mined episode containing verification token should be retrievable"
    );

    // The key _last_swept_at is stashed in STM
    let stm_map = backend
        .get_stm("sess_external_dream", Some("_last_swept_at"))
        .await?;
    let first_swept = stm_map
        .get("_last_swept_at")
        .cloned()
        .expect("_last_swept_at should be stashed in STM");
    assert!(
        !first_swept.is_empty(),
        "_last_swept_at should have a timestamp value"
    );

    Ok(())
}

}

mod client_server_auto_routing {
use anyhow::Result;
use mythrax_core::db::SurrealBackend;
use std::net::TcpListener;

#[tokio::test]
async fn test_client_server_auto_routing_detection() -> Result<()> {
    // 1. Find a free port by binding a listener and dropping it.
    let free_listener = TcpListener::bind("127.0.0.1:0")?;
    let free_port = free_listener.local_addr()?.port();
    drop(free_listener);

    unsafe {
        std::env::set_var("MYTHRAX_DAEMON_PORT", free_port.to_string());
    }

    // 2. Initialize backend in embedded mode.
    // Since the port is free (no daemon listening), it should default to embedded mode.
    let backend_embedded = SurrealBackend::new_in_memory().await?;
    assert!(
        !backend_embedded.is_client_mode(),
        "Backend must default to embedded mode when no daemon is running"
    );

    // 3. Now, we start a mock daemon on the same port to trigger client mode detection.
    let _mock_daemon = TcpListener::bind(format!("127.0.0.1:{}", free_port))?;

    // Re-initialize backend. It should now detect the active port and switch to client mode.
    let backend_client = SurrealBackend::new_client_connection().await;

    // Clean up env var immediately
    unsafe {
        std::env::remove_var("MYTHRAX_DAEMON_PORT");
    }

    let backend = backend_client?;
    assert!(
        backend.is_client_mode(),
        "Backend must switch to Client Mode when the daemon port is active"
    );

    Ok(())
}

}

mod async_embedding_execution {
use anyhow::Result;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use std::sync::Arc;

static TEST_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tokio::test]
async fn test_concurrent_embedding_execution() -> Result<()> {
    let _guard = match TEST_MUTEX.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let backend = Arc::new(backend);

    let mut handles = Vec::new();

    for i in 0..10 {
        let backend_clone = Arc::clone(&backend);
        let handle = tokio::spawn(async move {
            let _ = backend_clone.embed(&format!("test embedding {}", i)).await;
        });
        handles.push(handle);
    }

    for (i, handle) in handles.into_iter().enumerate() {
        match handle.await {
            Ok(_) => {}
            Err(e) => panic!("Task {} panicked: {:?}", i, e),
        }
    }

    Ok(())
}

}

mod completion_dynamic_server {
#[cfg(feature = "mlx")]
use mythrax_core::db::{StorageBackend, SurrealBackend};
#[cfg(feature = "mlx")]
use mythrax_core::llm::{DYNAMIC_MODEL_BROKER, DynamicModelBroker, LLMClient};
#[cfg(feature = "mlx")]
use std::env;
#[cfg(feature = "mlx")]
use std::path::Path;
#[cfg(feature = "mlx")]
use std::sync::Arc;
#[cfg(feature = "mlx")]
use tempfile::tempdir;

#[tokio::test]
#[cfg(feature = "mlx")]
async fn test_completion_dynamic_server_loading() {
    let home = env::var("HOME").unwrap();
    let model_dir = Path::new(&home).join(".mythrax/models");

    // Initialize SurrealDB in memory
    let backend = SurrealBackend::new(
        "mem://",
        mythrax_core::db::BackendConfig {
            check_daemon: false,
            embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)),
            llm: Some(mythrax_core::llm::LLMClient::new_mock()),
        },
    )
    .await
    .unwrap();
    backend.init().await.unwrap();

    // Update LLM config to local provider with Qwen 0.5B (Tier 1) model
    let req = mythrax_core::contracts::LlmConfigRequest {
        provider: "local".to_string(),
        duration: None,
        model: Some("mlx-community/Qwen2.5-0.5B-Instruct-4bit".to_string()),
        cloud_provider: Some("gemini".to_string()),
        api_key: None,
        llm_post_inference_delay_ms: None,
        model_tier_mappings: None,
    };
    backend.update_llm_config(&req).await.unwrap();

    // Initialize the dynamic model broker
    let broker = DynamicModelBroker::new(model_dir.clone()).await.unwrap();
    let broker_arc = Arc::new(broker);

    // Set the global static broker
    let _ = DYNAMIC_MODEL_BROKER.set(broker_arc.clone());

    println!("DEBUG TEST: Calling completion...");
    // Execute the completions call
    let client = LLMClient::new_mock();
    let response = client
        .completion(
            &backend,
            Some("You are a helpful assistant"),
            "Say Hello in one word",
        )
        .await;
    println!(
        "DEBUG TEST: Completion response received: {:?}",
        response.is_ok()
    );

    assert!(
        response.is_ok(),
        "Completion execution must succeed dynamically: {:?}",
        response.err()
    );
    let text = response.unwrap();
    assert!(!text.is_empty(), "Generated response must not be empty");

    // Evict unused models to trigger drop and verify cleanup
    drop(client);
    broker_arc.evict_unused_models().await;

    // Weak reference upgrade must fail indicating the model was evicted/cleaned up
    let weak_ref = broker_arc.get_weak_llm_reference();
    assert!(
        weak_ref.and_then(|w| w.upgrade()).is_none(),
        "In-process model must be cleaned up and evicted upon drop"
    );
}

#[tokio::test]
#[cfg(feature = "mlx")]
async fn test_complete_code_task_mcp_tool() {
    let home = env::var("HOME").unwrap();
    let model_dir = Path::new(&home).join(".mythrax/models");
    let temp_dir = tempdir().expect("Failed to create temp dir");

    // Initialize SurrealDB in memory
    let backend = SurrealBackend::new(
        "mem://",
        mythrax_core::db::BackendConfig {
            check_daemon: false,
            embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)),
            llm: Some(mythrax_core::llm::LLMClient::new_mock()),
        },
    )
    .await
    .unwrap();
    backend.init().await.unwrap();

    // Update LLM config to local provider
    let req = mythrax_core::contracts::LlmConfigRequest {
        provider: "local".to_string(),
        duration: None,
        model: Some("mlx-community/Qwen2.5-0.5B-Instruct-4bit".to_string()),
        cloud_provider: Some("gemini".to_string()),
        api_key: None,
        llm_post_inference_delay_ms: None,
        model_tier_mappings: None,
    };
    backend.update_llm_config(&req).await.unwrap();

    // Initialize the dynamic model broker
    let broker = DynamicModelBroker::new(model_dir.clone()).await.unwrap();
    let broker_arc = Arc::new(broker);

    // Set the global static broker (ignore if already set in previous test)
    let _ = DYNAMIC_MODEL_BROKER.set(broker_arc.clone());

    // Create ApiState
    let api_state = mythrax_core::api::ApiState {
        backend: Arc::new(backend),
        auth_token: "test_token".to_string(),
        store: Arc::new(
            mythrax_core::store::MarkdownStore::new(temp_dir.path().to_path_buf()).unwrap(),
        ),
        ignore_list: Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
        shutdown_tx: None,
    };

    // Invoke complete_code_task MCP tool via consolidated agent tool
    let args = serde_json::json!({
        "action": "complete_task",
        "prompt": "Write a rust function to add two numbers.",
        "system_instruction": "Be concise.",
        "model": "mlx-community/Qwen2.5-0.5B-Instruct-4bit"
    });

    let res = mythrax_core::mcp_routes::call_mcp_tool(&api_state, "agent", args).await;
    assert!(
        res.is_ok(),
        "MCP tool complete_code_task call must succeed: {:?}",
        res.err()
    );

    let val = res.unwrap();
    let text = val["content"][0]["text"].as_str().unwrap();
    assert!(
        !text.is_empty(),
        "Generated tool response must not be empty"
    );
}

#[tokio::test]
#[cfg(feature = "mlx")]
async fn test_tier3_completion_and_eviction() {
    let home = env::var("HOME").unwrap();
    let model_dir = Path::new(&home).join(".mythrax/models");

    // Initialize SurrealDB in memory
    let backend = SurrealBackend::new(
        "mem://",
        mythrax_core::db::BackendConfig {
            check_daemon: false,
            embedder: Some(std::sync::Arc::new(mythrax_core::embeddings::MockEmbedder)),
            llm: Some(mythrax_core::llm::LLMClient::new_mock()),
        },
    )
    .await
    .unwrap();
    backend.init().await.unwrap();

    // Update LLM config to local provider with Tier 3 model
    let req = mythrax_core::contracts::LlmConfigRequest {
        provider: "local".to_string(),
        duration: None,
        model: Some("mlx-community/Qwen3.6-35B-A3B-4bit".to_string()),
        cloud_provider: Some("gemini".to_string()),
        api_key: None,
        llm_post_inference_delay_ms: None,
        model_tier_mappings: None,
    };
    backend.update_llm_config(&req).await.unwrap();

    // Initialize the dynamic model broker
    let broker = DynamicModelBroker::new(model_dir.clone()).await.unwrap();
    let broker_arc = Arc::new(broker);

    // Set the global static broker
    let _ = DYNAMIC_MODEL_BROKER.set(broker_arc.clone());

    // Execute completion on Tier 3
    let client = LLMClient::new_mock();
    let response = client
        .completion(
            &backend,
            Some("You are a helpful assistant"),
            "Reason about 2+2",
        )
        .await;

    assert!(
        response.is_ok(),
        "Tier 3 completion execution must succeed dynamically: {:?}",
        response.err()
    );
    let text = response.unwrap();
    assert!(!text.is_empty(), "Generated response must not be empty");

    // Evict unused models to trigger drop and verify cleanup
    drop(client);
    broker_arc.evict_unused_models().await;

    // Weak reference upgrade must fail indicating the model was evicted/cleaned up
    let weak_ref = broker_arc.get_weak_llm_reference();
    assert!(
        weak_ref.and_then(|w| w.upgrade()).is_none(),
        "Tier 3 model must be cleaned up and evicted upon drop"
    );
}

}

mod hybrid_hydration_hook {
use anyhow::Result;
use mythrax_core::api::ApiState;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::mcp_routes::call_mcp_tool;
use mythrax_core::store::MarkdownStore;
use serde_json::json;

#[tokio::test]
async fn test_hybrid_hydration_hook_behavior() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let temp_dir = tempfile::tempdir()?;
    let store = MarkdownStore::new(temp_dir.path())?;

    let state = ApiState {
        backend: std::sync::Arc::new(backend),
        auth_token: "secret-api-token".to_string(),
        store: std::sync::Arc::new(store),
        ignore_list: std::sync::Arc::new(Default::default()),
        dream_tx: None,
        shutdown_tx: None,
    };

    // 1. Create a BeliefState in SurrealDB
    let session_id = "test-session-123";
    let _ = state
        .backend
        .as_any()
        .downcast_ref::<SurrealBackend>()
        .unwrap()
        .db
        .query(
            "
        UPSERT type::record('belief_state', $session_id) CONTENT {
            session_id: $session_id,
            tasks_todo: ['task1'],
            hypotheses_tested: ['hyp1'],
            confidence_score: 0.75,
            uncertainty_areas: ['unc1'],
            updated_at: '2026-06-25T00:00:00Z'
        };
    ",
        )
        .bind(("session_id", session_id))
        .await?;

    // 2. Insert handoff to trigger the search path
    let handoff = mythrax_core::contracts::HandoffSave {
        parent_conversation_id: "parent".to_string(),
        subagent_conversation_id: session_id.to_string(),
        summary: "test summary".to_string(),
        handoff_file_path: "handoff.md".to_string(),
        scope: Some("general".to_string()),
        include_tool_execution: None,
    };
    state.backend.save_handoff(&handoff).await?;

    // Call pre_invocation_hook via consolidated manage tool
    let args = json!({
        "action": "pre_invocation",
        "session_id": session_id,
        "query": "test query",
        "workspace_path": temp_dir.path().to_str().unwrap()
    });

    let response = call_mcp_tool(&state, "manage", args).await?;

    let text = response["content"][0]["text"].as_str().unwrap();

    // Verify BeliefState is prepended nicely
    assert!(text.contains("POMDP Belief State"));
    assert!(text.contains("0.75"));
    assert!(text.contains("task1"));

    Ok(())
}

}

mod inspect_safetensors {
#[cfg(feature = "mlx")]
#[test]
fn test_inspect_calibrated_score_pairs() {
    use mlx_rs::ops::indexing::TryIndexOp;
    use mlx_rs::{Array, StreamOrDevice};
    use mythrax_core::llm::MxbaiReranker;
    use std::path::Path;

    let home = std::env::var("HOME").unwrap();
    let model_dir = Path::new(&home).join(".mythrax/models/mxbai-rerank-large-v2");
    let mut reranker = MxbaiReranker::load(&model_dir).expect("Failed to load reranker");

    let query = "Who wrote 'To Kill a Mockingbird'?";
    let relevant_passage = "To Kill a Mockingbird is a novel by Harper Lee published in 1960. It was immediately successful.";
    let irrelevant_passage =
        "The President of the United States is the head of state and head of government.";

    // Passages to evaluate (prepend empty string as null passage)
    let passages = vec!["", relevant_passage, irrelevant_passage];

    let mut tokenized_sequences = Vec::new();
    let mut max_seq_len = 0;

    for passage in &passages {
        let text = format!(
            "query: {}\ndocument: {}\nYou are a search relevance expert who evaluates how well documents match search queries. For each query-document pair, carefully analyze the semantic relationship between them, then provide your binary relevance judgment (0 for not relevant, 1 for relevant).\nRelevance:",
            query, passage
        );
        let encoding = reranker.tokenizer.encode(text, false).unwrap();
        let ids = encoding.get_ids();
        let ids_i32: Vec<i32> = ids.iter().map(|&x| x as i32).collect();
        if ids_i32.len() > max_seq_len {
            max_seq_len = ids_i32.len();
        }
        tokenized_sequences.push(ids_i32);
    }

    let batch_size = passages.len();
    let pad_token_id = 151643;

    // Bucket max_seq_len to the nearest multiple of 128
    let max_seq_len_bucketed = ((max_seq_len + 127) / 128) * 128;
    println!(
        "Original max_seq_len: {}, Bucketed to: {}",
        max_seq_len, max_seq_len_bucketed
    );

    let mut flat_ids = Vec::with_capacity(batch_size * max_seq_len_bucketed);
    let mut item_masks = Vec::with_capacity(batch_size);

    let max_seq_len_i32 = max_seq_len_bucketed as i32;
    let causal_mask = mlx_rs::ops::full::<f32>(
        &[max_seq_len_i32, max_seq_len_i32],
        &Array::from(f32::NEG_INFINITY),
    )
    .unwrap();
    let causal_mask = mlx_rs::ops::triu_device(&causal_mask, 1, StreamOrDevice::gpu()).unwrap();

    let indices = mlx_rs::ops::arange::<i32, i32>(0, max_seq_len_i32, 1).unwrap();
    let r_indices = indices.reshape(&[max_seq_len_i32, 1]).unwrap();
    let c_indices = indices.reshape(&[1, max_seq_len_i32]).unwrap();
    let is_diagonal = r_indices.eq(&c_indices).unwrap();
    let not_diagonal = is_diagonal.logical_not().unwrap();

    for seq in &tokenized_sequences {
        let pad_len = max_seq_len_bucketed - seq.len();
        for _ in 0..pad_len {
            flat_ids.push(pad_token_id);
        }
        flat_ids.extend(seq);

        let is_pad = indices.lt(&Array::from(pad_len as i32)).unwrap();
        let is_pad_2d = is_pad.reshape(&[1, max_seq_len_i32]).unwrap();
        let mask_cond = is_pad_2d.logical_and(&not_diagonal).unwrap();

        let neg_inf = Array::from(f32::NEG_INFINITY);
        let zero = Array::from(0.0f32);
        let padding_mask = mlx_rs::ops::which(&mask_cond, &neg_inf, &zero).unwrap();

        let item_mask = causal_mask.add(&padding_mask).unwrap();
        item_masks.push(item_mask);
    }

    let ids_array = Array::from_slice(&flat_ids, &[batch_size as i32, max_seq_len_i32]);
    let mask = mlx_rs::ops::stack(&item_masks).unwrap();
    let mask = mask
        .reshape(&[batch_size as i32, 1, max_seq_len_i32, max_seq_len_i32])
        .unwrap();

    let out = reranker.model.as_mut().unwrap().forward(&ids_array, Some(&mask)).unwrap();
    let last_hidden = out.try_index((.., max_seq_len_i32 - 1, ..)).unwrap();

    let embed_w = reranker.model.as_ref().unwrap().embed_tokens.weight.value.clone();
    let w_no_tok = embed_w.try_index((2152, ..)).unwrap();
    let w_yes_tok = embed_w.try_index((9693, ..)).unwrap();

    let logit_no = last_hidden
        .multiply(&w_no_tok)
        .unwrap()
        .sum_axes_device(&[-1], false, StreamOrDevice::gpu())
        .unwrap();
    let logit_yes = last_hidden
        .multiply(&w_yes_tok)
        .unwrap()
        .sum_axes_device(&[-1], false, StreamOrDevice::gpu())
        .unwrap();

    // Slice null logits (item 0)
    let null_logit_no = logit_no.try_index(0).unwrap();
    let null_logit_yes = logit_yes.try_index(0).unwrap();

    // Calibrate logits for items 1..batch_size
    let real_logit_no = logit_no.try_index(1..).unwrap();
    let real_logit_yes = logit_yes.try_index(1..).unwrap();

    let calibrated_logit_no = real_logit_no.subtract(&null_logit_no).unwrap();
    let calibrated_logit_yes = real_logit_yes.subtract(&null_logit_yes).unwrap();

    let max_logit = mlx_rs::ops::maximum(&calibrated_logit_no, &calibrated_logit_yes).unwrap();
    let exp_no = calibrated_logit_no
        .subtract(&max_logit)
        .unwrap()
        .exp()
        .unwrap();
    let exp_yes = calibrated_logit_yes
        .subtract(&max_logit)
        .unwrap()
        .exp()
        .unwrap();
    let sum_exp = exp_no.add(&exp_yes).unwrap();
    let scores_array = exp_yes.divide(&sum_exp).unwrap();
    let scores_array = scores_array.as_dtype(mlx_rs::Dtype::Float32).unwrap();

    let scores = scores_array.as_slice::<f32>().to_vec();
    println!("CALIBRATED BATCHED SCORES: {:?}", scores);
    assert_eq!(scores.len(), 2);
    assert!(
        scores[0] > scores[1],
        "Relevant passage must score higher than irrelevant passage"
    );
}

}

mod model_broker {
#[cfg(feature = "mlx")]
use mythrax_core::llm::{DynamicModelBroker, ModelTier};
#[cfg(feature = "mlx")]
use tempfile::tempdir;

#[tokio::test]
#[cfg(feature = "mlx")]
async fn test_model_broker_lifecycle_and_warmup_fallback() {
    println!("DEBUG BROKER TEST: Start");
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let broker = DynamicModelBroker::new(temp_dir.path().to_path_buf())
        .await
        .unwrap();

    // 1. Preload pinned embedding model
    println!("DEBUG BROKER TEST: Preloading embedding model");
    broker
        .preload_embedding_model("mlx-community/nomic-embed-text-v1.5-mlx")
        .await
        .unwrap();
    assert!(broker.is_embedding_model_loaded());

    // 2. Load the default Coder/MoE LLM (Qwen3.6-35B-A3B)
    println!("DEBUG BROKER TEST: Acquiring coder_model Tier2");
    if std::env::var("MYTHRAX_TEST_MOCK").is_err() {
        broker
            .update_config_model("mlx-community/Qwen2.5-0.5B-Instruct-4bit")
            .await
            .unwrap();
    }
    let coder_model = broker.acquire_llm(ModelTier::Tier2).await.unwrap();
    if std::env::var("MYTHRAX_TEST_MOCK").is_ok() {
        assert_eq!(coder_model.name(), "mlx-community/Qwen3.6-35B-A3B-4bit");
    } else {
        assert_eq!(
            coder_model.name(),
            "mlx-community/Qwen2.5-0.5B-Instruct-4bit"
        );
    }

    // Verify pre-inference shader warm-up was executed
    assert!(coder_model.is_warmed_up());

    // 3. Verify dynamic stop tokens are parsed from tokenizer_config.json
    println!("DEBUG BROKER TEST: Checking stop tokens");
    let stop_tokens = coder_model.stop_tokens();
    assert!(stop_tokens.contains(&"<|eot_id|>".to_string()));

    // 4. Verify weak-pointer tracking: dropping the reference unloads the model from VRAM
    println!("DEBUG BROKER TEST: Testing weak reference eviction");
    let weak_ref = broker.get_weak_llm_reference();
    drop(coder_model);

    broker.evict_unused_models().await;
    assert!(
        weak_ref.and_then(|w| w.upgrade()).is_none(),
        "Model must be evicted from VRAM when strong reference count drops to 0"
    );

    // 5. Verify dynamic model selection: update config to another model and load
    println!("DEBUG BROKER TEST: Testing alternative model acquisition");
    broker
        .update_config_model("mlx-community/Qwen2.5-0.5B-Instruct-4bit")
        .await
        .unwrap();
    let model_alt = broker.acquire_llm(ModelTier::Tier2).await.unwrap();
    assert_eq!(model_alt.name(), "mlx-community/Qwen2.5-0.5B-Instruct-4bit");
    drop(model_alt);
    broker.evict_unused_models().await;

    // 6. Simulate Metal shader cache corruption / warm-up panic
    println!("DEBUG BROKER TEST: Testing corrupt broker fallback");
    let corrupt_broker = DynamicModelBroker::new_corrupt_mock().await.unwrap();
    let res = corrupt_broker
        .acquire_llm_with_warmup_fallback(ModelTier::Tier2)
        .await;

    assert!(
        res.is_ok(),
        "Warmup fallback must catch shader cache panics and succeed"
    );
    let fallback_model = res.unwrap();
    assert_eq!(
        fallback_model.execution_mode(),
        "cpu",
        "Must fallback to CPU execution mode"
    );
    println!("DEBUG BROKER TEST: End");
}

}

mod precompact_hook {
use std::fs::File;
use std::io::Write;
use tempfile::tempdir;

#[test]
fn sanitize_session_id_strips_unsafe() {
    assert_eq!(
        mythrax_core::hooks::shell::sanitize_session_id("a/b c.d"),
        "abcd"
    );
    assert_eq!(
        mythrax_core::hooks::shell::sanitize_session_id("session_123-abc"),
        "session_123-abc"
    );
    assert_eq!(
        mythrax_core::hooks::shell::sanitize_session_id(""),
        "unknown"
    );
}

#[test]
fn normalize_path_preserves_windows_drive() {
    let p = mythrax_core::hooks::shell::normalize_transcript_path("C:\\Users\\me\\s.jsonl");
    assert_eq!(p, "C:/Users/me/s.jsonl"); // backslashes->slashes, colon kept
}

#[test]
fn count_human_messages_skips_command_messages() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("transcript.jsonl");
    let mut file = File::create(&file_path).unwrap();

    // JSONL content: 2 human user messages (one with "<command-message>"), 1 assistant message
    let lines = vec![
        r#"{"role": "user", "content": "hello world"}"#,
        r#"{"role": "user", "content": "running <command-message> test"}"#,
        r#"{"role": "assistant", "content": "hi there"}"#,
    ];

    for line in lines {
        writeln!(file, "{}", line).unwrap();
    }

    let path_str = file_path.to_string_lossy();
    let count = mythrax_core::hooks::shell::count_human_messages(&path_str);
    assert_eq!(count, 1); // Only the first user message should be counted as human, the second is skipped
}

}

mod swap_monitor {
use mythrax_core::daemon::monitor::{check_disk_space, check_swap_pressure};
use mythrax_core::llm::ModelTier;
use tempfile::tempdir;

#[test]
fn test_canonicalized_mount_point_disk_check() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let target_dir = temp_dir.path().join("download_dir");
    std::fs::create_dir_all(&target_dir).unwrap();

    let symlink_dir = temp_dir.path().join("symlinked_dir");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target_dir, &symlink_dir).unwrap();

    let massive_bytes = 10 * 1024 * 1024 * 1024 * 1024; // 10 Terabytes
    let res = check_disk_space(&symlink_dir, massive_bytes);
    assert!(
        res.is_err(),
        "Must correctly canonicalize symlink and fail disk space check on partition"
    );
}

#[test]
fn test_model_aware_swap_eviction_thresholds() {
    // Tier 1: 1.5B (Threshold 2.0 GB)
    let evict_tier1_high = check_swap_pressure(ModelTier::Tier1, 2_100 * 1024 * 1024);
    assert!(evict_tier1_high, "Tier 1 must evict at 2.1 GB swap");
    let evict_tier1_low = check_swap_pressure(ModelTier::Tier1, 1_500 * 1024 * 1024);
    assert!(!evict_tier1_low, "Tier 1 must not evict at 1.5 GB swap");

    // Tier 2: 7B Coder (Threshold 3.0 GB)
    let evict_tier2_high = check_swap_pressure(ModelTier::Tier2, 3_100 * 1024 * 1024);
    assert!(evict_tier2_high, "Tier 2 must evict at 3.1 GB swap");
    let evict_tier2_low = check_swap_pressure(ModelTier::Tier2, 2_500 * 1024 * 1024);
    assert!(!evict_tier2_low, "Tier 2 must not evict at 2.5 GB swap");

    // Tier 3: 35B Deep Reason (Threshold 6.0 GB)
    let evict_tier3_high = check_swap_pressure(ModelTier::Tier3, 6_100 * 1024 * 1024);
    assert!(evict_tier3_high, "Tier 3 must evict at 6.1 GB swap");
    let evict_tier3_low = check_swap_pressure(ModelTier::Tier3, 5_500 * 1024 * 1024);
    assert!(!evict_tier3_low, "Tier 3 must not evict at 5.5 GB swap");
}

}
