use std::fs;
use anyhow::Result;
use tempfile::tempdir;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::cognitive::compactor::Compactor;
use mythrax_core::store::MarkdownStore;

use std::sync::Mutex;
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_compactor_decay_referenced_safety() -> Result<()> {
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
    let store = MarkdownStore::new(&vault_root)?;
    let compactor = Compactor::new();

    // 1. Create a referenced episode that is decayed:
    let ep_save = EpisodeSave {
        created_at: None,
        title: "Referenced Episode".to_string(),
        content: "Some important referenced content.".to_string(),
        scope: Some("general".to_string()),
        vault_path: Some("episodes/referenced_ep.md".to_string()),
        ..Default::default()
    };
    let ep_id = backend.save_episode(&ep_save).await?;

    // Manually set utility to 2.0 to force decay
    let ep_raw_id = ep_id.split(':').nth(1).unwrap_or(&ep_id).to_string();
    backend.db.query("UPDATE type::record('episode', $id) SET utility = 2.0;")
        .bind(("id", ep_raw_id.clone()))
        .await?.check()?;

    // Create the physical file
    store.write_file("episodes/referenced_ep.md", "Some important referenced content.")?;

    // Let's relate this episode to a wiki node so it is referenced
    let node_contract = mythrax_core::contracts::WikiNode {
        id: Some("wiki_node:target_node".to_string()),
        name: "Target Node".to_string(),
        content: "some content".to_string(),
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
    };
    backend.save_wiki_node(&node_contract).await?;
    backend.relate_nodes(&ep_id, "wiki_node:target_node", None, None, None).await?;

    // Verify it is referenced in the DB
    let is_ref = {
        let ep_rec = mythrax_core::db::backend::parse_record_id(&ep_id)?;
        let mut resp = backend.db.query("SELECT VALUE id FROM relates_to WHERE in = $ep OR out = $ep LIMIT 1;").bind(("ep", ep_rec)).await?;
        let rows: Vec<surrealdb::types::RecordId> = resp.take(0)?;
        !rows.is_empty()
    };
    assert!(is_ref);

    // Call compaction to trigger decay of this node
    let _ = compactor.compact_scope(&backend, &store, "general", None).await;

    // Check if the physical file still exists in its original place
    let orig_file = vault_root.join("episodes/referenced_ep.md");
    assert!(orig_file.exists(), "Referenced episode physical file must be preserved");

    // Check if it is marked as archived in the DB
    let mut resp = backend.db.query("SELECT archived FROM type::record('episode', $id);")
        .bind(("id", ep_raw_id))
        .await?;
    let rows: Vec<serde_json::Value> = resp.take(0)?;
    let archived = rows[0].get("archived").and_then(|v| v.as_bool()).unwrap_or(false);
    assert!(archived, "Referenced episode must be marked as archived in DB");

    Ok(())
}
