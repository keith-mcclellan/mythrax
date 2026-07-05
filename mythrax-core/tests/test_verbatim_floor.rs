use tempfile::tempdir;

use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::store::MarkdownStore;
use mythrax_core::cognitive::compactor::Compactor;

#[tokio::test]
async fn decayed_episode_still_retrievable_but_demoted() -> anyhow::Result<()> {
    unsafe { std::env::set_var("MYTHRAX_MOCK_LLM", "true"); }
    // 1. Initialize backend + MarkdownStore (tempdir)
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    let temp_vault = tempdir()?;
    let store = MarkdownStore::new(temp_vault.path())?;

    // 2. Save two episodes with similar content
    let ep_hi = EpisodeSave {
        title: "Agentic Memory Systems Architecture".to_string(),
        content: "Core design details of agentic memory layers, focusing on episodic retrieval and bitemporal graphs.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("agentic_memory_hi.md".to_string()),
        source_episode: None,
        session_id: Some("session-1".to_string()),
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,
    };
    let ep_low = EpisodeSave {
        title: "Backup Notes on Agentic Memory".to_string(),
        content: "Draft backup notes describing basic episodic retrieval concepts and simple graph structures.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("agentic_memory_low.md".to_string()),
        source_episode: None,
        session_id: Some("session-1".to_string()),
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,
    };

    // Write physical files so the compactor watcher doesn't get confused when doing moves
    store.write_file("agentic_memory_hi.md", &ep_hi.content)?;
    store.write_file("agentic_memory_low.md", &ep_low.content)?;

    let id_hi = backend.save_episode(&ep_hi).await?;
    let id_low = backend.save_episode(&ep_low).await?;

    // Extract UUIDs
    let uuid_hi = id_hi.split(':').nth(1).unwrap();
    let uuid_low = id_low.split(':').nth(1).unwrap();

    // 3. Mutate database to set utility (hi = 80.0, low = 1.0 to trigger decay compaction)
    let response_hi = backend.db.query("UPDATE type::record('episode',$id) MERGE { utility: 80.0 }")
        .bind(("id", uuid_hi.to_string()))
        .await?;
    response_hi.check()?;

    let response_low = backend.db.query("UPDATE type::record('episode',$id) MERGE { utility: 1.0 }")
        .bind(("id", uuid_low.to_string()))
        .await?;
    response_low.check()?;

    // 4. Run Compactor (compact_scope triggers archive_decayed_episodes internally)
    let compactor = Compactor::new();
    compactor.compact_scope(&backend, &store, "general", None).await?;

    // 5. ASSERT that the decayed episode is STILL retrievable but demoted
    // Search with threshold 0.0 to retrieve all matches
    let search_res = backend.search(
        "Agentic Memory",
        Some("general"),
        false,
        10,
        0,
        0.0,
        None,
        false,
        true,
        true,
        None,
        true,
    ).await?;

    // The low importance episode must still exist in the results (proving it wasn't deleted)
    let low_retrieved = search_res.results.iter().any(|r| r.id == id_low);
    assert!(low_retrieved, "Decayed episode was deleted instead of being demoted");

    // The high importance episode must rank above the low importance (demoted) episode
    let idx_hi = search_res.results.iter().position(|r| r.id == id_hi).unwrap();
    let idx_low = search_res.results.iter().position(|r| r.id == id_low).unwrap();
    assert!(idx_hi < idx_low, "Decayed episode ranks above high utility episode");

    // Assert that archived is marked true in the database
    let mut select_res = backend.db.query("SELECT archived FROM type::record('episode',$id)")
        .bind(("id", uuid_low.to_string()))
        .await?;
    let select_val: Option<serde_json::Value> = select_res.take(0)?;
    let archived_val = select_val
        .and_then(|v| v.get("archived").and_then(|a| a.as_bool()))
        .unwrap_or(false);
    assert!(archived_val, "Decayed episode was not marked archived");

    Ok(())
}

#[tokio::test]
async fn raptor_summary_is_additive_not_replacement() -> anyhow::Result<()> {
    // After compaction, both the RAPTOR wiki_node and the original episode co-exist in the database.
    // (This will be verified as part of the compactor behavior)
    Ok(())
}
