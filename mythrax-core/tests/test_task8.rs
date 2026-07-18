use std::fs::File;
use std::io::Write;
use std::sync::Arc;
use tempfile::tempdir;

use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::WatchIgnoreList;
use mythrax_core::llm::LLMClient;

#[tokio::test]
async fn test_checkpoint_resume() -> anyhow::Result<()> {
    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let vault_dir = tempdir()?;
    let store = Arc::new(MarkdownStore::new(vault_dir.path())?);
    let ignore = WatchIgnoreList::new();

    let trans_dir = tempdir()?;
    let transcript_path = trans_dir.path().join("transcript.jsonl");
    let mut trans_file = File::create(&transcript_path)?;
    let turns = vec![
        r#"{"role": "user", "content": "Turn 1 content"}"#,
        r#"{"role": "user", "content": "Turn 2 content"}"#,
        r#"{"role": "user", "content": "Turn 3 content"}"#,
        r#"{"role": "user", "content": "Turn 4 content"}"#,
    ];

    for turn in turns {
        writeln!(trans_file, "{}", turn)?;
    }

    let path_str = transcript_path.to_string_lossy();

    let count1 = mythrax_core::hooks::precompact::mine_transcript(
        "sess_checkpoint_test",
        &path_str,
        backend.as_ref(),
        &store,
        &ignore,
    ).await?;
    assert_eq!(count1, 4);

    let checkpoint_dir = store.vault_root.join(".mythrax");
    std::fs::create_dir_all(&checkpoint_dir)?;
    let checkpoint_path = checkpoint_dir.join("bootstrap_checkpoint.json");
    
    let checkpoint_json = r#"{"session_id": "sess_checkpoint_test", "last_processed_index": 1}"#;
    std::fs::write(&checkpoint_path, checkpoint_json)?;

    let _ = backend.clear_stm("sess_checkpoint_test").await;

    let count2 = mythrax_core::hooks::precompact::mine_transcript(
        "sess_checkpoint_test",
        &path_str,
        backend.as_ref(),
        &store,
        &ignore,
    ).await?;
    assert_eq!(count2, 2);

    Ok(())
}

#[tokio::test]
async fn test_quota_exhaustion_hibernation() -> anyhow::Result<()> {
    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let client = LLMClient::new();
    let profile = mythrax_core::contracts::TaskProfile::new(mythrax_core::contracts::TaskArchetype::Reasoning);

    unsafe {
        std::env::set_var("GEMINI_API_KEY", "dummy_key");
        std::env::remove_var("MYTHRAX_FORCE_LOCAL");
        std::env::remove_var("MYTHRAX_TEST_MOCK");
        std::env::set_var("MYTHRAX_MOCK_FAIL", "true");
        std::env::set_var("MYTHRAX_QUOTA_RETRY_SECS", "1");
    }

    let res1 = client.routed_completion(backend.as_ref(), &profile, None, "test").await;
    assert!(res1.is_err());
    assert!(!mythrax_core::llm::is_hibernating());

    let res2 = client.routed_completion(backend.as_ref(), &profile, None, "test").await;
    assert!(res2.is_err());
    assert!(!mythrax_core::llm::is_hibernating());

    let start = std::time::Instant::now();
    let res3 = client.routed_completion(backend.as_ref(), &profile, None, "test").await;
    assert!(res3.is_err());
    assert!(start.elapsed() >= std::time::Duration::from_secs(1));

    unsafe {
        std::env::remove_var("MYTHRAX_MOCK_FAIL");
        std::env::remove_var("MYTHRAX_QUOTA_RETRY_SECS");
        std::env::remove_var("GEMINI_API_KEY");
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
    }
    Ok(())
}