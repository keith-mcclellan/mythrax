use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::cognitive::synthesis::DreamCoordinator;
use mythrax_core::store::MarkdownStore;

#[tokio::test]
async fn test_paginated_dreaming_loop_boundary() -> Result<()> {
    let tmp = tempdir()?;
    let vault_root = tmp.path().join("vault");
    fs::create_dir_all(&vault_root)?;
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom"))?;
    fs::create_dir_all(vault_root.join("episodes"))?;

    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;
    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    let store = MarkdownStore::new(&vault_root)?;
    let coordinator = DreamCoordinator::new();

    // Seed 10 mock episodes.
    for i in 0..10 {
        let ep = EpisodeSave {
            session_id: Some("test_session".to_string()),
            title: format!("Episode {}", i),
            content: format!("Content description for episode {}", i),
            scope: Some("general".to_string()),
            ..Default::default()
        };
        let ep_id = backend.save_episode(&ep).await?;
        
        // Update its embedding in SurrealDB directly
        backend.db.query("UPDATE $id SET embedding = $emb;")
            .bind(("id", mythrax_core::db::backend::parse_record_id(&ep_id)?))
            .bind(("emb", vec![1.0; 768]))
            .await?.check()?;
    }

    // Assert that we have 10 unprocessed episodes before dreaming.
    let unprocessed_before = backend.get_unprocessed_episodes().await?;
    assert_eq!(unprocessed_before.len(), 10);

    // Run dreaming
    coordinator.run_dream(&backend, &store, Some("incremental"), None).await?;

    // Assert all 10 are now marked processed.
    let unprocessed_after = backend.get_unprocessed_episodes().await?;
    assert_eq!(unprocessed_after.len(), 0);

    unsafe {
        std::env::remove_var("MYTHRAX_WORKSPACE_ROOT");
        std::env::remove_var("MYTHRAX_MOCK_LLM");
    }

    Ok(())
}
