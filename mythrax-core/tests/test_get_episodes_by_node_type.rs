use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::EpisodeSave;

use std::sync::Mutex;
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_get_episodes_by_node_type_filtering() -> Result<()> {
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

    let workspace_root = tmp.path().join("workspace");
    fs::create_dir_all(&workspace_root)?;
    unsafe {
        std::env::remove_var("MYTHRAX_VAULT_ROOT");
        std::env::set_var("MYTHRAX_WORKSPACE_ROOT", workspace_root.to_str().unwrap());
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create a procedural episode
    let ep_proc = EpisodeSave {
        title: "Procedural Ep".to_string(),
        content: "Some procedural content".to_string(),
        scope: Some("test_scope".to_string()),
        ..Default::default()
    };
    let proc_id = backend.save_episode(&ep_proc).await?;
    let proc_raw_id = proc_id.split(':').nth(1).unwrap().to_string();
    backend.db.query("UPDATE type::record('episode', $id) SET node_type = 'procedural';")
        .bind(("id", proc_raw_id))
        .await?.check()?;

    // Create a standard episode
    let ep_std = EpisodeSave {
        title: "Standard Ep".to_string(),
        content: "Some standard content".to_string(),
        scope: Some("test_scope".to_string()),
        ..Default::default()
    };
    let std_id = backend.save_episode(&ep_std).await?;
    let std_raw_id = std_id.split(':').nth(1).unwrap().to_string();
    backend.db.query("UPDATE type::record('episode', $id) SET node_type = 'standard';")
        .bind(("id", std_raw_id))
        .await?.check()?;

    // Retrieve episodes by node type
    let proc_episodes = backend.get_episodes_by_node_type("procedural").await?;
    assert_eq!(proc_episodes.len(), 1);
    assert_eq!(proc_episodes[0].title, "Procedural Ep");

    let std_episodes = backend.get_episodes_by_node_type("standard").await?;
    assert_eq!(std_episodes.len(), 1);
    assert_eq!(std_episodes[0].title, "Standard Ep");

    Ok(())
}
