use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::vault::ingestion::bulk_ingest_vault;
use std::sync::Mutex;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_bulk_ingest_positional_correction() -> Result<()> {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;
    fs::create_dir_all(vault_root.join("episodes"))?;

    let source_dir = tmp.path().join("source");
    let session_dir = source_dir.join("session_123");
    let logs_dir = session_dir.join(".system_generated/logs");
    fs::create_dir_all(&logs_dir)?;

    // Write a mock transcript with multiple user turns containing a correction keyword in the second turn
    let jsonl_content = r#"{"type": "USER_INPUT", "content": "Please implement a simple calculator.", "created_at": "2026-07-19T21:00:00Z"}
{"type": "PLANNER_RESPONSE", "content": "Here is the code for the calculator.", "created_at": "2026-07-19T21:00:05Z"}
{"type": "USER_INPUT", "content": "Wait, you forgot to handle division by zero!", "created_at": "2026-07-19T21:00:10Z"}
{"type": "PLANNER_RESPONSE", "content": "I'm sorry, let me fix that.", "created_at": "2026-07-19T21:00:15Z"}
"#;
    fs::write(logs_dir.join("transcript.jsonl"), jsonl_content)?;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Perform bulk ingestion with skip_llm = true
    let (count, errors, has_more) = bulk_ingest_vault(
        &vault_root,
        &source_dir,
        "antigravity",
        "general",
        &backend,
        None,
        None,
        true,
    ).await?;

    assert_eq!(count, 1, "Should ingest 1 episode session");
    assert!(errors.is_empty(), "Should ingest without errors");
    assert!(!has_more);

    // Verify if a cognitive task was enqueued for the correction keyword ("forgot")
    let pending_tasks = backend.get_pending_cognitive_tasks().await?;
    assert_eq!(pending_tasks.len(), 1, "Should enqueue exactly 1 cognitive task for the correction");
    assert!(pending_tasks[0].prompt.contains("forgot to handle division by zero"), "Task prompt should contain the correction content");

    Ok(())
}
