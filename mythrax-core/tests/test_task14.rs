use tempfile::tempdir;
use std::fs;
use std::sync::Arc;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::hooks::reflect::handle_reflect;

#[tokio::test]
async fn test_reflect_queues_cognitive_task() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("db");
    let backend = SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy()), mythrax_core::db::BackendConfig {
        check_daemon: false,
        embedder: Some(Arc::new(mythrax_core::embeddings::MockEmbedder)),
        llm: Some(mythrax_core::llm::LLMClient::new_mock()),
    }).await.unwrap();
    backend.init().await.unwrap();

    // Create a mock transcript file with 10 turns and 5 tool calls to pass the gate
    let transcript_path = temp_dir.path().join("transcript.jsonl");
    let mut transcript_content = String::new();
    for i in 0..10 {
        let role = if i % 2 == 0 { "user" } else { "assistant" };
        let tool_call = if i < 5 { r#", "tool_calls": [{"name": "read_file"}]"# } else { "" };
        transcript_content.push_str(&format!(
            r#"{{"step_index": {}, "source": "MODEL", "type": "PLANNER_RESPONSE", "content": "hello turn {}"{}}}"#,
            i, i, tool_call
        ));
        transcript_content.push_str("\n");
    }
    fs::write(&transcript_path, transcript_content).unwrap();

    let session_id = "test_session_123";
    let status = handle_reflect(session_id, &transcript_path.to_string_lossy(), &backend).await.unwrap();
    
    // Assert task queued
    assert_eq!(status, "reflection_queued");

    // Fetch pending tasks
    let pending = backend.get_pending_cognitive_tasks().await.unwrap();
    assert_eq!(pending.len(), 1);
    let task = &pending[0];
    assert_eq!(task.task_type, "reflection_distillation");
    assert_eq!(task.session_id.as_deref(), Some(session_id));
}

#[tokio::test]
async fn test_reflect_skips_trivial_sessions() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("db");
    let backend = SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy()), mythrax_core::db::BackendConfig {
        check_daemon: false,
        embedder: Some(Arc::new(mythrax_core::embeddings::MockEmbedder)),
        llm: Some(mythrax_core::llm::LLMClient::new_mock()),
    }).await.unwrap();
    backend.init().await.unwrap();

    // Trivial transcript: 2 turns, 1 tool call
    let transcript_path = temp_dir.path().join("trivial.jsonl");
    fs::write(&transcript_path, r#"{"type":"USER_INPUT","content":"hi"}
{"type":"PLANNER_RESPONSE","content":"hello","tool_calls":[{"name":"read"}]}
"#).unwrap();

    let status = handle_reflect("session_trivial", &transcript_path.to_string_lossy(), &backend).await.unwrap();
    assert_eq!(status, "skipped_trivial");

    let pending = backend.get_pending_cognitive_tasks().await.unwrap();
    assert!(pending.is_empty());
}

#[tokio::test]
async fn test_reflect_transcript_missing() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("db");
    let backend = SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy()), mythrax_core::db::BackendConfig {
        check_daemon: false,
        embedder: Some(Arc::new(mythrax_core::embeddings::MockEmbedder)),
        llm: Some(mythrax_core::llm::LLMClient::new_mock()),
    }).await.unwrap();
    backend.init().await.unwrap();

    // Transcript doesn't exist
    let status = handle_reflect("session_missing", "/nonexistent/path.jsonl", &backend).await.unwrap();
    assert_eq!(status, "skipped_missing");
}

#[tokio::test]
async fn test_harvest_completed_reflections() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("db");
    let backend = SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy()), mythrax_core::db::BackendConfig {
        check_daemon: false,
        embedder: Some(Arc::new(mythrax_core::embeddings::MockEmbedder)),
        llm: Some(mythrax_core::llm::LLMClient::new_mock()),
    }).await.unwrap();
    backend.init().await.unwrap();

    let session_id = "test_session_harvest";
    
    let task_id = format!("cognitive_task:{}", uuid::Uuid::new_v4());
    let result_json = serde_json::json!({
        "outcome": "failure",
        "causal_explanation": "Ran out of tokens",
        "lessons": ["Monitor token usage"],
        "error_patterns": ["Max length exceeded"],
        "files_modified": ["src/lib.rs"]
    });
    
    let task = mythrax_core::db::CognitiveTask {
        id: task_id.clone(),
        task_type: "reflection_distillation".to_string(),
        prompt: "distill...".to_string(),
        system_instruction: "sys".to_string(),
        expected_format: "Json".to_string(),
        priority: "Normal".to_string(),
        created_at: chrono::Utc::now(),
        status: "Completed".to_string(),
        result: Some(serde_json::to_string(&result_json).unwrap()),
        ttl_minutes: 10,
        injected_at: None,
        session_id: Some(session_id.to_string()),
    };
    
    backend.create_cognitive_task(&task).await.unwrap();
    
    let rule = mythrax_core::contracts::WisdomRule {
        id: None,
        target_pattern: "Failed session approach".to_string(),
        action_to_avoid: "Repeat failed approach".to_string(),
        causal_explanation: "Existing causal".to_string(),
        prescribed_remedy: "Existing remedy".to_string(),
        tier: "working".to_string(),
        scope: "general".to_string(),
        vault_path: None,
        source_episodes: vec![],
        generator_name: "reflect_harvester".to_string(),
        embedding: Some(vec![1.0; 384]), 
        utility: Some(50.0),
        status: Some("active".to_string()),
        superseded_at: None,
        superseded_by: None,
        severity: Some("low".to_string()),
        blocking: Some(false),
        rule_type: Some("pruned_hypothesis".to_string()),
    };
    backend.save_wisdom_rule_db(&rule).await.unwrap();

    mythrax_core::hooks::reflect::harvest_completed_reflections(&backend).await.unwrap();
    
    let sql = "SELECT * FROM type::thing('cognitive_task', $id);";
    let id_part = task_id.strip_prefix("cognitive_task:").unwrap();
    let mut res = backend.db.query(sql).bind(("id", id_part)).await.unwrap();
    let tasks: Vec<mythrax_core::db::CognitiveTask> = res.take(0).unwrap();
    assert!(tasks.is_empty(), "Processed task should be deleted");
    
    let ep_sql = "SELECT * FROM episode WHERE session_id = $session_id;";
    let mut ep_res = backend.db.query(ep_sql).bind(("session_id", session_id)).await.unwrap();
    let eps: Vec<serde_json::Value> = ep_res.take(0).unwrap();
    assert_eq!(eps.len(), 1);
    assert_eq!(eps[0]["node_type"], "experience");
    
    let rule_sql = "SELECT * FROM wisdom WHERE rule_type = 'pruned_hypothesis';";
    let mut rule_res = backend.db.query(rule_sql).await.unwrap();
    let rules: Vec<mythrax_core::contracts::WisdomRule> = rule_res.take(0).unwrap();
    assert!(rules.iter().any(|r| r.utility.unwrap_or(0.0) > 50.0));
}
