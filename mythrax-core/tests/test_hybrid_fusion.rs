use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::contracts::EpisodeSave;

#[tokio::test]
async fn test_hybrid_fusion_toggle() -> anyhow::Result<()> {
    unsafe { std::env::set_var("MYTHRAX_MOCK_LLM", "true"); }
    
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    
    // Save some episodes
    // Ep 1: strong lexical match for "basement pipes", but unrelated to vector topic
    let ep1 = EpisodeSave {
        title: "basement pipes".to_string(),
        content: "Draft notes about rusty metal pipes located in the old cold basement.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: None,
        source_episode: None,
        session_id: Some("sess-1".to_string()),
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
    };
    
    // Ep 2: strong semantic match for "artificial intelligence", but no mention of "basement pipes"
    let ep2 = EpisodeSave {
        title: "agentic systems".to_string(),
        content: "Deep research on neural architectures and advanced agentic memory consolidation models.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: None,
        source_episode: None,
        session_id: Some("sess-1".to_string()),
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
    };
    
    let id1 = backend.save_episode(&ep1).await?;
    let _id2 = backend.save_episode(&ep2).await?;
    
    // 1. Search with hybrid OFF (default/vector-only)
    // Query: "basement pipes"
    // With hybrid OFF, the vector search is performed.
    backend.save_profile_key("retrieval.hybrid", "false").await?;
    let _res_off = backend.search("basement pipes", Some("general"), false, 5, 0, 0.0, None, false, true, true).await?;
    
    // 2. Search with hybrid ON
    backend.save_profile_key("retrieval.hybrid", "true").await?;
    let res_on = backend.search("basement pipes", Some("general"), false, 5, 0, 0.0, None, false, true, true).await?;
    
    // If hybrid is ON, the lexical match (Ep 1) should rank highly (and be returned as a high-scoring result)
    // because of its 100% lexical term overlap.
    assert!(!res_on.results.is_empty(), "Hybrid search should return results");
    let found_ep1_on = res_on.results.iter().any(|r| r.id == id1);
    assert!(found_ep1_on, "Hybrid search should return the lexically matching episode");
    
    Ok(())
}
