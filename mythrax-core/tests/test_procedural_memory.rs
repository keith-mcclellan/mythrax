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
async fn test_procedural_memory_decay_and_cap() -> Result<()> {
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

    // 1. Verify 365-day half-life protection for procedural nodes:
    // Create a procedural episode and a standard episode, both 100 days old.
    let hundred_days_ago = (chrono::Utc::now() - chrono::Duration::days(100)).to_rfc3339();

    // Procedural
    let ep_proc = EpisodeSave {
        title: "Procedural Ep".to_string(),
        content: "Some procedural content".to_string(),
        scope: Some("test_scope".to_string()),
        vault_path: Some("episodes/proc.md".to_string()),
        ..Default::default()
    };
    let proc_id = backend.save_episode(&ep_proc).await?;
    store.write_file("episodes/proc.md", "Some procedural content")?;
    let proc_raw_id = proc_id.split(':').nth(1).unwrap().to_string();
    backend.db.query("UPDATE type::record('episode', $id) SET node_type = 'procedural', last_retrieved_at = $lr, utility = 50.0;")
        .bind(("id", proc_raw_id.clone()))
        .bind(("lr", hundred_days_ago.clone()))
        .await?.check()?;

    // Standard (not procedural)
    let ep_std = EpisodeSave {
        title: "Standard Ep".to_string(),
        content: "Some standard content".to_string(),
        scope: Some("test_scope".to_string()),
        vault_path: Some("episodes/std.md".to_string()),
        ..Default::default()
    };
    let std_id = backend.save_episode(&ep_std).await?;
    store.write_file("episodes/std.md", "Some standard content")?;
    let std_raw_id = std_id.split(':').nth(1).unwrap().to_string();
    backend.db.query("UPDATE type::record('episode', $id) SET last_retrieved_at = $lr, utility = 50.0;")
        .bind(("id", std_raw_id.clone()))
        .bind(("lr", hundred_days_ago.clone()))
        .await?.check()?;

    // Run prune_stale_memories (or compact_scope) to trigger decay evaluation
    compactor.compact_scope(&backend, &store, "test_scope", None).await?;

    // Verify Standard Ep is archived
    let mut resp = backend.db.query("SELECT archived FROM type::record('episode', $id);")
        .bind(("id", std_raw_id))
        .await?;
    let rows: Vec<serde_json::Value> = resp.take(0)?;
    let std_archived = rows[0].get("archived").and_then(|v| v.as_bool()).unwrap_or(false);
    assert!(std_archived, "Standard episode should be archived after 100 days");

    // Verify Procedural Ep is NOT archived
    let mut resp = backend.db.query("SELECT archived FROM type::record('episode', $id);")
        .bind(("id", proc_raw_id))
        .await?;
    let rows: Vec<serde_json::Value> = resp.take(0)?;
    let proc_archived = rows[0].get("archived").and_then(|v| v.as_bool()).unwrap_or(false);
    assert!(!proc_archived, "Procedural episode should NOT be archived after 100 days");

    // 2. Verify 500-node LRU cap per scope:
    // Insert 505 procedural episodes in a new scope
    for k in 0..505 {
        let ep = EpisodeSave {
            title: format!("Proc Cap {}", k),
            content: format!("Content {}", k),
            scope: Some("cap_scope".to_string()),
            vault_path: Some(format!("episodes/cap_{}.md", k)),
            ..Default::default()
        };
        let eid = backend.save_episode(&ep).await?;
        let eraw = eid.split(':').nth(1).unwrap().to_string();
        
        // We set last_retrieved_at to be sequentially increasing, so older ones are evicted first.
        let time_str = (chrono::Utc::now() - chrono::Duration::hours(505 - k)).to_rfc3339();
        backend.db.query("UPDATE type::record('episode', $id) SET node_type = 'procedural', last_retrieved_at = $lr;")
            .bind(("id", eraw))
            .bind(("lr", time_str))
            .await?.check()?;
    }

    // Run pruning
    compactor.compact_scope(&backend, &store, "cap_scope", None).await?;

    // Query active (unarchived) procedural episodes in cap_scope
    let mut resp = backend.db.query("SELECT * FROM episode WHERE scope = 'cap_scope' AND node_type = 'procedural' AND (archived = false OR archived IS NONE);").await?;
    let active_cap_eps: Vec<serde_json::Value> = resp.take(0)?;
    assert_eq!(active_cap_eps.len(), 500, "Active procedural episodes in cap_scope should be capped at 500");

    // Assert that the oldest 5 (Cap 0 to Cap 4) are archived
    for k in 0..5 {
        let mut resp = backend.db.query("SELECT archived FROM episode WHERE title = $title LIMIT 1;")
            .bind(("title", format!("Proc Cap {}", k)))
            .await?;
        let rows: Vec<serde_json::Value> = resp.take(0)?;
        let archived = rows[0].get("archived").and_then(|v| v.as_bool()).unwrap_or(false);
        assert!(archived, "Episode Proc Cap {} (one of the oldest) should be archived", k);
    }

    Ok(())
}
