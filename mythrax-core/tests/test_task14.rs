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
