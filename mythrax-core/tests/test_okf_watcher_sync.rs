use anyhow::Result;
use tempfile::tempdir;
use std::fs;
use std::sync::Arc;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::db::backend::format_record_id;
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::{start_watching, sync_file_to_db, WatchIgnoreList};

fn calculate_hash<T: Hash>(t: &T) -> u64 {
    let mut s = DefaultHasher::new();
    t.hash(&mut s);
    s.finish()
}

#[tokio::test]
async fn test_okf_watcher_differential_sync_and_loop_prevention() -> Result<()> {
    // 1. Initialize DB and Vault Store in a temporary directory
    let backend: Arc<dyn StorageBackend> = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let tmp = tempdir()?;
    let vault_root = tmp.path().to_path_buf();
    let store = Arc::new(MarkdownStore::new(vault_root.clone())?);

    // Create subdirectories for episodes, wisdom, and wiki
    fs::create_dir_all(vault_root.join("wiki"))?;
    fs::create_dir_all(vault_root.join("wisdom/skills"))?;

    // Start the watcher
    let ignore_list = Arc::new(WatchIgnoreList::new());
    let _watcher = start_watching(
        vault_root.clone(),
        ignore_list.clone(),
        backend.clone(),
        store.clone(),
        None,
    )?;

    // 2. Create target note (Page B)
    ignore_list.ignore(vault_root.join("wiki/page_b.md"));
    let page_b_content = "---\nname: Page B\nscope: general\n---\n# Page B content\n";
    fs::write(vault_root.join("wiki/page_b.md"), page_b_content)?;
    
    // Explicitly trigger sync for the test to ensure execution
    sync_file_to_db(&vault_root.join("wiki/page_b.md"), &backend, &store).await?;

    // We can query using backend directly by downcasting or using its query method if exposed by StorageBackend.
    // In our case, StorageBackend has search/relate methods, but we can downcast to SurrealBackend in the test to run raw queries.
    let surreal_backend = backend.as_any().downcast_ref::<SurrealBackend>().unwrap();

    let mut res_b = surreal_backend.db.query("SELECT VALUE id FROM wiki_node WHERE name = 'Page B' LIMIT 1;").await?;
    let id_b: Option<surrealdb::types::RecordId> = res_b.take(0)?;
    assert!(id_b.is_some(), "Page B node must be synced to database");
    let id_b_str = format_record_id(id_b.as_ref().unwrap());

    // 3. Create source note (Page A) with Obsidian YAML edges and body wikilinks
    let page_a_content = format!(
        r#"---
name: Page A
scope: general
importance: 7.5
edges:
  - target: "[[{}]]"
    relation: "supersedes"
    strength: 0.95
---
# Page A
This is a link to [[Page B]] in the body.
"#,
        id_b_str
    );
    ignore_list.ignore(vault_root.join("wiki/page_a.md"));
    fs::write(vault_root.join("wiki/page_a.md"), page_a_content)?;
    
    // Sync Page A
    sync_file_to_db(&vault_root.join("wiki/page_a.md"), &backend, &store).await?;

    let mut res_a = surreal_backend.db.query("SELECT VALUE id FROM wiki_node WHERE name = 'Page A' LIMIT 1;").await?;
    let id_a: Option<surrealdb::types::RecordId> = res_a.take(0)?;
    assert!(id_a.is_some(), "Page A node must be synced to database");
    let id_a_str = format_record_id(id_a.as_ref().unwrap());

    // 4. Verify that relations were successfully created
    // There should be a "supersedes" edge (from frontmatter) and a "related" relates_to edge (from body wikilink)
    let mut rel_query = surreal_backend.db.query("SELECT relation, strength, out FROM relates_to WHERE in = $from;")
        .bind(("from", id_a.as_ref().unwrap().clone()))
        .await?;
    let relations: Vec<serde_json::Value> = rel_query.take(0)?;
    assert_eq!(relations.len(), 2, "Should create exactly 2 relations: 1 from frontmatter, 1 from body wikilink");

    let relations_types: Vec<String> = relations.iter().map(|r| r["relation"].as_str().unwrap().to_string()).collect();
    assert!(relations_types.contains(&"supersedes".to_string()));
    assert!(relations_types.contains(&"related".to_string()));

    // 5. Test Differential Sync: Update Page A to remove the frontmatter edge
    let page_a_updated = r#"---
name: Page A
scope: general
importance: 7.5
edges: []
---
# Page A
This is a link to [[Page B]] in the body.
"#;
    ignore_list.ignore(vault_root.join("wiki/page_a.md"));
    fs::write(vault_root.join("wiki/page_a.md"), page_a_updated)?;
    
    // Re-sync Page A
    sync_file_to_db(&vault_root.join("wiki/page_a.md"), &backend, &store).await?;

    // Verify that the "supersedes" relation was deleted, but the body wikilink relation remains
    let mut rel_query_2 = surreal_backend.db.query("SELECT relation, out FROM relates_to WHERE in = $from;")
        .bind(("from", id_a.as_ref().unwrap().clone()))
        .await?;
    let relations_2: Vec<serde_json::Value> = rel_query_2.take(0)?;
    assert_eq!(relations_2.len(), 1, "Should prune deleted relation, leaving only 1 relation");
    assert_eq!(relations_2[0]["relation"], "related");

    // 6. A-MEM Split-Brain Mitigation: Verify that watcher sync preserves dynamically decayed cognitive metadata
    // Simulate dynamic decay and retrieval updates in database
    surreal_backend.db.query("UPDATE type::record('wiki_node', $id) MERGE { utility: 12.5, last_retrieved_at: '2026-06-24T00:00:00Z' };")
        .bind(("id", id_a_str.split(':').nth(1).unwrap()))
        .await?.check()?;

    // Sync Page A again. Since sync uses UPDATE ... MERGE, it should NOT reset utility and last_retrieved_at to default
    sync_file_to_db(&vault_root.join("wiki/page_a.md"), &backend, &store).await?;

    let mut select_metadata = surreal_backend.db.query("SELECT utility, last_retrieved_at FROM type::record('wiki_node', $id);")
        .bind(("id", id_a_str.split(':').nth(1).unwrap()))
        .await?;
    let metadata: Option<serde_json::Value> = select_metadata.take(0)?;
    assert!(metadata.is_some());
    let m_val = metadata.unwrap();
    assert_eq!(m_val["utility"].as_f64().unwrap(), 12.5, "Sync must preserve dynamic decayed utility score (Split-Brain Mitigation)");
    assert_eq!(m_val["last_retrieved_at"].as_str().unwrap(), "2026-06-24T00:00:00Z", "Sync must preserve dynamic retrieval timestamps");

    // 7. Verify Content-Hash ignore suppressor
    // Write content and register hash in ignore list
    let content = "Check ignore suppressor";
    let hash = calculate_hash(&content);
    ignore_list.ignore_hash(hash);

    // Watcher should ignore this event if the file is modified with this content
    assert!(ignore_list.is_hash_ignored(&hash), "Hash must be registered as ignored");

    Ok(())
}
