use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::{EpisodeSave, WikiNode};

#[tokio::test]
async fn test_node_type_differentiation() -> Result<()> {
    unsafe { std::env::set_var("MYTHRAX_MOCK_LLM", "true"); }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    backend.save_profile_key("search.enable_graph_expansion", "true").await?;
    backend.save_profile_key("search.temporal_depth", "2").await?;

    // Create Episode -> WikiNode -> Episode
    let ep1 = EpisodeSave {
        title: "Episode 1".to_string(),
        content: "First episode".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let ep1_id = backend.save_episode(&ep1).await?;

    let node = WikiNode {
        id: None,
        name: "Middle Node".to_string(),
        content: "Middle node".to_string(),
        scope: "general".to_string(),
        ..Default::default()
    };
    let node_id = backend.save_wiki_node(&node).await?;

    let ep2 = EpisodeSave {
        title: "Episode 2".to_string(),
        content: "Target episode".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let ep2_id = backend.save_episode(&ep2).await?;

    // Relate them: ep1 -> node -> ep2 via followed_by
    let query = "RELATE $from -> followed_by -> $to;";
    
    backend.db.query(query)
        .bind(("from", mythrax_core::db::parse_record_id(&ep1_id)?))
        .bind(("to", mythrax_core::db::parse_record_id(&node_id)?))
        .await?.check()?;

    backend.db.query(query)
        .bind(("from", mythrax_core::db::parse_record_id(&node_id)?))
        .bind(("to", mythrax_core::db::parse_record_id(&ep2_id)?))
        .await?.check()?;

    // Search for "First episode" to trigger expansion which should traverse through WikiNode to Episode 2
    let resp = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "First episode",
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
        false,
        None,
    )).await?;

    // Find ep2 in results
    let found = resp.results.iter().any(|r| r.id == ep2_id);
    assert!(found, "Should find ep2 through the wiki_node in the temporal chain");

    Ok(())
}
