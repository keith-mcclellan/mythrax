use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::EpisodeSave;

#[tokio::test]
async fn test_concept_prefilter_narrows_candidates() {
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    // 1. Close but untagged episode
    let ep_close = EpisodeSave {
        created_at: None,
        title: "Security and authentication overview".to_string(),
        content: "This document describes overall security patterns including auth and tokens.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/security.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: Some(vec!["security".to_string()]),
        files_read: None,
        files_modified: None,
        node_type: None,
    
        confidence: None,};
    backend.save_episode(&ep_close).await.unwrap();

    // 2. Target episode tagged with "oauth"
    let ep_target = EpisodeSave {
        created_at: None,
        title: "OAuth setup guide".to_string(),
        content: "Steps to configure the oauth provider and client secrets. security patterns".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/oauth.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: Some(vec!["oauth".to_string(), "security".to_string()]),
        files_read: None,
        files_modified: None,
        node_type: None,
    
        confidence: None,};
    let target_id = backend.save_episode(&ep_target).await.unwrap();

    // 3. Unrelated episode
    let ep_unrelated = EpisodeSave {
        created_at: None,
        title: "Database schema migrations".to_string(),
        content: "SurrealDB tables and indexes for belief states and thought nodes.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/db.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: Some(vec!["database".to_string()]),
        files_read: None,
        files_modified: None,
        node_type: None,
    
        confidence: None,};
    backend.save_episode(&ep_unrelated).await.unwrap();

    // Search for concept "oauth" - should return only the target episode
    let res = backend.search_filtered(
        "security patterns",
        Some("general"),
        10,
        0.0,
        &["oauth".to_string()],
        &[]
    ).await.unwrap();

    assert_eq!(res.results.len(), 1);
    assert_eq!(res.results[0].id, target_id);
}

#[tokio::test]
async fn test_files_modified_filter() {
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    let ep1 = EpisodeSave {
        created_at: None,
        title: "Fix compiler errors in api.rs".to_string(),
        content: "Fixed struct literals and stand-alone commas in api.rs. refactored tests or fixes".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/api_fix.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: Some(vec!["api.rs".to_string()]),
        node_type: None,
    
        confidence: None,};
    let id1 = backend.save_episode(&ep1).await.unwrap();

    let ep2 = EpisodeSave {
        created_at: None,
        title: "Update backend tests".to_string(),
        content: "Refactored tests/test_temporal_edges.rs to verify edge invalidations. refactored tests or fixes".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/test_fix.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: Some(vec!["test_temporal_edges.rs".to_string()]),
        node_type: None,
    
        confidence: None,};
    backend.save_episode(&ep2).await.unwrap();

    // Search filtered by file "api.rs"
    let res = backend.search_filtered(
        "refactored tests or fixes",
        Some("general"),
        10,
        0.0,
        &[],
        &["api.rs".to_string()]
    ).await.unwrap();

    assert_eq!(res.results.len(), 1);
    assert_eq!(res.results[0].id, id1);
}

#[tokio::test]
async fn test_structured_filter_never_empties_floor() {
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    let ep = EpisodeSave {
        created_at: None,
        title: "General note".to_string(),
        content: "Some general content here.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/gen.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: Some(vec!["general".to_string()]),
        files_read: None,
        files_modified: None,
        node_type: None,
    
        confidence: None,};
    let id = backend.save_episode(&ep).await.unwrap();

    // Search with a concept that doesn't exist ("nonexistent")
    // It should fall back to unfiltered results instead of returning empty list!
    let res = backend.search_filtered(
        "General note",
        Some("general"),
        10,
        0.0,
        &["nonexistent".to_string()],
        &[]
    ).await.unwrap();

    assert!(!res.results.is_empty());
    assert_eq!(res.results[0].id, id);
}
