use anyhow::Result;
use mythrax_core::cognitive::compactor::Compactor;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use std::fs;
use tempfile::tempdir;

use std::sync::Mutex;
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_near_duplicate_merging_behavior() -> Result<()> {
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

    // Enable feature and set threshold
    backend
        .save_profile_key("compactor.enable_near_duplicate_merging", "true")
        .await?;
    backend
        .save_profile_key("compactor.dedup_threshold", "0.90")
        .await?;

    let embedding = vec![1.0f32; 768];

    // Create older episode
    let ep_older = EpisodeSave {
        title: "Older Episode".to_string(),
        content: "Older content".to_string(),
        scope: Some("test_scope".to_string()),
        vault_path: Some("episodes/older.md".to_string()),
        session_id: Some("session-123".to_string()),
        ..Default::default()
    };
    let older_id = backend.save_episode(&ep_older).await?;
    store.write_file("episodes/older.md", "Older content")?;

    // Manually set embedding and last_retrieved_at to be older
    let older_raw_id = older_id.split(':').nth(1).unwrap().to_string();
    backend.db.query("UPDATE type::record('episode', $id) SET embedding = $emb, last_retrieved_at = '2026-07-05T10:00:00Z', node_type = 'test_type';")
        .bind(("id", older_raw_id.clone()))
        .bind(("emb", embedding.clone()))
        .await?.check()?;

    // Update metrics with access count = 5 for older
    backend
        .db
        .query(
            "UPDATE metrics SET access_count = 5 WHERE target_id = type::record('episode', $id);",
        )
        .bind(("id", older_raw_id.clone()))
        .await?
        .check()?;

    // Create newer episode
    let ep_newer = EpisodeSave {
        title: "Newer Episode".to_string(),
        content: "Newer content".to_string(),
        scope: Some("test_scope".to_string()),
        vault_path: Some("episodes/newer.md".to_string()),
        session_id: Some("session-123".to_string()),
        ..Default::default()
    };
    let newer_id = backend.save_episode(&ep_newer).await?;
    store.write_file("episodes/newer.md", "Newer content")?;

    // Manually set embedding and last_retrieved_at to be newer
    let newer_raw_id = newer_id.split(':').nth(1).unwrap().to_string();
    backend.db.query("UPDATE type::record('episode', $id) SET embedding = $emb, last_retrieved_at = '2026-07-05T12:00:00Z', node_type = 'test_type';")
        .bind(("id", newer_raw_id.clone()))
        .bind(("emb", embedding.clone()))
        .await?.check()?;

    // Update metrics with access count = 3 for newer
    backend
        .db
        .query(
            "UPDATE metrics SET access_count = 3 WHERE target_id = type::record('episode', $id);",
        )
        .bind(("id", newer_raw_id.clone()))
        .await?
        .check()?;

    // Run compact_scope
    compactor
        .compact_scope(&backend, &store, "test_scope", None)
        .await?;

    // Verify newer episode is deleted
    let mut resp = backend
        .db
        .query("SELECT * FROM type::record('episode', $id);")
        .bind(("id", newer_raw_id.clone()))
        .await?;
    let rows: Vec<serde_json::Value> = resp.take(0)?;
    assert!(rows.is_empty(), "Newer episode should be deleted from DB");

    // Verify newer physical file is deleted
    let newer_file = vault_root.join("episodes/newer.md");
    assert!(
        !newer_file.exists(),
        "Newer physical file should be deleted"
    );

    // Verify older episode has merged content
    let mut resp = backend
        .db
        .query("SELECT content FROM type::record('episode', $id);")
        .bind(("id", older_raw_id.clone()))
        .await?;
    let rows: Vec<serde_json::Value> = resp.take(0)?;
    let content = rows[0].get("content").and_then(|v| v.as_str()).unwrap();
    assert_eq!(
        content, "Older content\nNewer content",
        "Content should be merged"
    );

    // Verify older physical file has merged content
    let older_file_content = fs::read_to_string(vault_root.join("episodes/older.md"))?;
    assert_eq!(older_file_content, "Older content\nNewer content");

    // Verify older metrics has access count = 8 (5 + 3)
    let mut resp = backend
        .db
        .query("SELECT access_count FROM metrics WHERE target_id = type::record('episode', $id);")
        .bind(("id", older_raw_id))
        .await?;
    let rows: Vec<serde_json::Value> = resp.take(0)?;
    let access_count = rows[0]
        .get("access_count")
        .and_then(|v| v.as_i64())
        .unwrap();
    assert_eq!(access_count, 8);

    Ok(())
}
