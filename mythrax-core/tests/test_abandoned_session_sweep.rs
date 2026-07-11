use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use tempfile::tempdir;
use std::time::Duration;
use tokio::time::sleep;

use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;

#[tokio::test]
async fn test_abandoned_session_sweep_lifecycle() -> anyhow::Result<()> {
    // Set up mock environment variables like test_compactor.rs
    let trans_dir = tempdir()?;
    let workspace_path = trans_dir.path().join("workspace");
    std::fs::create_dir_all(&workspace_path)?;
    
    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_path.to_str().unwrap());
        if std::env::var("MYTHRAX_TEST_MOCK").is_ok() {
            std::env::set_var("MYTHRAX_MOCK_LLM", "true");
        } else {
            std::env::set_var("MYTHRAX_MOCK_LLM", "false");
        }
    }

    #[cfg(feature = "mlx")]
    {
        if std::env::var("MYTHRAX_TEST_MOCK").is_err() {
            let home = std::env::var("HOME").unwrap();
            let models_dir = std::path::PathBuf::from(home).join(".mythrax/models");
            let broker = mythrax_core::llm::DynamicModelBroker::new(models_dir).await.unwrap();
            let _ = mythrax_core::llm::DYNAMIC_MODEL_BROKER.set(Arc::new(broker));
        }
    }

    // 1. Build in-memory backend + MarkdownStore(tempdir)
    let backend: Arc<dyn StorageBackend> = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let vault_dir = tempdir()?;
    let store = MarkdownStore::new(vault_dir.path())?;

    // 2. Create the transcript directory & file
    let transcript_path = trans_dir.path().join("transcript.jsonl");
    let transcript_path_str = transcript_path.to_string_lossy().to_string();

    let mut trans_file = File::create(&transcript_path)?;
    writeln!(trans_file, r#"{{"role": "user", "content": "Execute test command"}}"#)?;
    writeln!(trans_file, r#"{{"role": "tool", "content": "Command finished successfully: SWEEP_TEST_VERIFICATION_TOKEN"}}"#)?;
    drop(trans_file);

    // 3. Register the transcript path in STM
    backend.save_stm("sess_abandoned", "_transcript_path", &transcript_path_str).await?;
    backend.save_stm("sess_abandoned", "_last_activity", "some activity").await?;

    // 4. Force aging of STM records to satisfy >10m idleness check
    let surreal_backend = backend.as_any().downcast_ref::<SurrealBackend>()
        .expect("Failed to downcast to SurrealBackend");
    surreal_backend.db
        .query("UPDATE short_term_memory SET updated_at = time::now() - 11m WHERE session_id = 'sess_abandoned';")
        .await?
        .check()?;

    // 5. Run the compactor dreaming sweep
    let coordinator = mythrax_core::cognitive::synthesis::DreamCoordinator::new();
    coordinator.run_dream(&*backend, &store, Some("incremental"), None).await?;

    // Assertion 1: Verify the new turns are mined into the database
    let search_res = backend.search(
        "SWEEP_TEST_VERIFICATION_TOKEN",
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
    ).await?;
    assert!(search_res.total_matches > 0, "Mined episode containing verification token should be retrievable");

    // Assertion 2: The key _last_swept_at is stashed in STM
    let stm_map = backend.get_stm("sess_abandoned", Some("_last_swept_at")).await?;
    let first_swept = stm_map.get("_last_swept_at").cloned()
        .expect("_last_swept_at should be stashed in STM");
    assert!(!first_swept.is_empty(), "_last_swept_at should have a timestamp value");

    // Assertion 3: The key _transcript_path remains registered
    let stm_map = backend.get_stm("sess_abandoned", Some("_transcript_path")).await?;
    let registered_path = stm_map.get("_transcript_path").cloned()
        .expect("_transcript_path should still be registered");
    assert_eq!(registered_path, transcript_path_str);

    // 6. Test No-Op Sweep: Run the sweep again without modifying the file
    // We update last activity again to be idle so it gets swept
    surreal_backend.db
        .query("UPDATE short_term_memory SET updated_at = time::now() - 11m WHERE session_id = 'sess_abandoned';")
        .await?
        .check()?;

    coordinator.run_dream(&*backend, &store, Some("incremental"), None).await?;

    // Verify _last_swept_at was NOT updated (same timestamp string as first)
    let stm_map = backend.get_stm("sess_abandoned", Some("_last_swept_at")).await?;
    let second_swept = stm_map.get("_last_swept_at").cloned().unwrap_or_default();
    assert_eq!(first_swept, second_swept, "Should not update _last_swept_at if transcript is unmodified");

    // 7. Test Modified File Sweep: Modify the file, update idle, and verify it sweep-mines again
    // Sleep briefly to ensure the file system modification time changes
    sleep(Duration::from_millis(1100)).await;

    let mut trans_file = std::fs::OpenOptions::new()
        .append(true)
        .open(&transcript_path)?;
    writeln!(trans_file, r#"{{"role": "user", "content": "Execute second test command"}}"#)?;
    writeln!(trans_file, r#"{{"role": "tool", "content": "Command finished successfully: ADDITIONAL_TEST_TOKEN"}}"#)?;
    drop(trans_file);

    // Make it idle again
    surreal_backend.db
        .query("UPDATE short_term_memory SET updated_at = time::now() - 11m WHERE session_id = 'sess_abandoned';")
        .await?
        .check()?;

    coordinator.run_dream(&*backend, &store, Some("incremental"), None).await?;

    // Assert that the new content was mined
    let search_res = backend.search(
        "ADDITIONAL_TEST_TOKEN",
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
    ).await?;
    assert!(search_res.total_matches > 0, "Second mined episode should be retrievable");

    // Assert that _last_swept_at has changed/updated
    let stm_map = backend.get_stm("sess_abandoned", Some("_last_swept_at")).await?;
    let third_swept = stm_map.get("_last_swept_at").cloned().unwrap_or_default();
    assert_ne!(second_swept, third_swept, "_last_swept_at timestamp should have updated on new modifications");

    // 8. Test Missing File Cleanup: Delete the transcript file and verify registration is deleted
    std::fs::remove_file(&transcript_path)?;

    // Make it idle again
    surreal_backend.db
        .query("UPDATE short_term_memory SET updated_at = time::now() - 11m WHERE session_id = 'sess_abandoned';")
        .await?
        .check()?;

    coordinator.run_dream(&*backend, &store, Some("incremental"), None).await?;

    // Assert that the registry was cleaned up (STM keys cleared)
    let stm_map = backend.get_stm("sess_abandoned", None).await?;
    assert!(stm_map.get("_transcript_path").is_none(), "_transcript_path registry key should be deleted");
    assert!(stm_map.get("_last_swept_at").is_none(), "_last_swept_at registry key should be deleted");

    Ok(())
}
