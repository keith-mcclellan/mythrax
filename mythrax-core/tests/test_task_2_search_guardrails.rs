use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend, parse_record_id};
use mythrax_core::contracts::{EpisodeSave, SearchParams};
use mythrax_core::mcp_routes::strip_diffs;
use chrono::Utc;

#[tokio::test]
async fn test_standard_search_filters_conflict_nodes() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // 1. Insert conflict node (node_type = 'conflict')
    let ep_conflict = EpisodeSave {
        title: "Conflict Episode".to_string(),
        content: "This represents a code/rule conflict.".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let id_conflict = backend.save_episode(&ep_conflict).await?;
    let uuid_conflict = id_conflict.split(':').nth(1).unwrap();
    backend.db.query("UPDATE type::record('episode', $id) SET node_type = 'conflict', importance = 8.0;")
        .bind(("id", uuid_conflict))
        .await?.check()?;

    // 2. Insert standard node (node_type = 'standard')
    let ep_standard = EpisodeSave {
        title: "Standard Episode".to_string(),
        content: "This is standard text content.".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let id_standard = backend.save_episode(&ep_standard).await?;
    let uuid_standard = id_standard.split(':').nth(1).unwrap();
    backend.db.query("UPDATE type::record('episode', $id) SET node_type = 'standard', importance = 5.0;")
        .bind(("id", uuid_standard))
        .await?.check()?;

    // 3. Perform standard search (query = "standard text") -> should NOT return conflict node
    let search_params_std = SearchParams {
        query: "standard text".to_string(),
        scope: Some("general".to_string()),
        limit: 10,
        include_episodes: true,
        ..Default::default()
    };
    let resp_std = backend.search(search_params_std).await?;
    let has_conflict = resp_std.results.iter().any(|r| r.id == id_conflict);
    assert!(!has_conflict, "Conflict nodes must be excluded from standard search");

    // 4. Perform exploratory search (query contains "conflict") -> should return conflict node
    let search_params_exp = SearchParams {
        query: "resolving conflict".to_string(),
        scope: Some("general".to_string()),
        limit: 10,
        include_episodes: true,
        ..Default::default()
    };
    let resp_exp = backend.search(search_params_exp).await?;
    let has_conflict_exp = resp_exp.results.iter().any(|r| r.id == id_conflict);
    assert!(has_conflict_exp, "Conflict nodes must be retrieved in exploratory queries");

    Ok(())
}

#[tokio::test]
async fn test_standard_search_hides_pending_htr_nodes() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create hypothesis_node Root
    backend.db.query("CREATE type::record('hypothesis_node', 'root_node') CONTENT { node_id: 'root_node', status: 'pending', hypothesis: 'base root hypothesis', score: 50.0 };").await?.check()?;
    
    // Create an episode
    let ep = EpisodeSave {
        title: "HTR Episode".to_string(),
        content: "HTR execution trace episode content.".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let ep_id = backend.save_episode(&ep).await?;

    // Relate episode -> hypothesis_node
    let from_id = parse_record_id(&ep_id)?;
    let to_id = parse_record_id("hypothesis_node:root_node")?;
    backend.db.query("RELATE $from -> relates_to -> $to;")
        .bind(("from", from_id))
        .bind(("to", to_id))
        .await?.check()?;

    // Search with deep_insight = true. Since the hypothesis node is pending, it should be hidden from related nodes
    let search_params = SearchParams {
        query: "HTR execution trace".to_string(),
        scope: Some("general".to_string()),
        deep_insight: true,
        include_episodes: true,
        limit: 10,
        ..Default::default()
    };
    let resp = backend.search(search_params.clone()).await?;
    let ep_res = resp.results.iter().find(|r| r.id == ep_id).unwrap();
    
    // Check if related_nodes list contains the hypothesis node
    if let Some(ref related) = ep_res.related_nodes {
        let has_pending = related.iter().any(|r| r.id.contains("root_node"));
        assert!(!has_pending, "Pending HTR hypothesis node must be hidden from related nodes");
    }

    // Now complete the HTR node (status = 'done')
    backend.db.query("UPDATE type::record('hypothesis_node', 'root_node') SET status = 'done';").await?.check()?;
    let resp_done = backend.search(search_params).await?;
    let ep_res_done = resp_done.results.iter().find(|r| r.id == ep_id).unwrap();
    let related = ep_res_done.related_nodes.as_ref().expect("related nodes list should be present");
    let has_done = related.iter().any(|r| r.id.contains("root_node"));
    assert!(has_done, "Completed HTR hypothesis node should be visible in related nodes");

    Ok(())
}

#[test]
fn test_diff_strip_format_methods() {
    let content = "Hello world\ndiff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n-old\n+new\n```diff\n-removed\n+added\n```\nSome footer text";
    let stripped = strip_diffs(content);
    assert!(!stripped.contains("diff --git"), "Should strip raw diff header");
    assert!(!stripped.contains("--- a/"), "Should strip diff old file path");
    assert!(!stripped.contains("+++ b/"), "Should strip diff new file path");
    assert!(!stripped.contains("@@ -"), "Should strip diff chunk header");
    assert!(!stripped.contains("removed"), "Should strip code inside diff block");
    assert!(stripped.contains("Hello world"), "Should keep Hello world");
    assert!(stripped.contains("Some footer text"), "Should keep non-diff text");
    assert!(stripped.contains("[Diff Truncated]"), "Should insert Diff Truncated placeholder");
}

#[tokio::test]
async fn test_temporal_decay_uses_temporal_range_end() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Insert two wiki nodes
    // Node A: created 10 days ago, but temporal_range_end is now
    let now = Utc::now();
    let ten_days_ago = now - chrono::Duration::days(10);
    
    let emb = vec![1.0; 768];
    // We create them directly
    backend.db.query("CREATE type::record('wiki_node', 'node_a') CONTENT { name: 'Node A', content: 'Node A content', scope: 'general', created_at: $created_a, temporal_range_end: $temp_end_a, importance: 5.0, embedding: $emb };")
        .bind(("created_a", ten_days_ago))
        .bind(("temp_end_a", now))
        .bind(("emb", emb.clone()))
        .await?.check()?;

    // Node B: created 10 days ago, no temporal_range_end
    backend.db.query("CREATE type::record('wiki_node', 'node_b') CONTENT { name: 'Node B', content: 'Node B content', scope: 'general', created_at: $created_b, importance: 5.0, embedding: $emb };")
        .bind(("created_b", ten_days_ago))
        .bind(("emb", emb))
        .await?.check()?;

    // Search for "Node content" using a temporal search context
    let search_params = SearchParams {
        query: "content".to_string(),
        scope: Some("general".to_string()),
        limit: 10,
        temporal_anchor: Some(now.to_rfc3339()),
        threshold: 0.0,
        ..Default::default()
    };
    
    let resp = backend.search(search_params).await?;
    let results = resp.results;
    
    let score_a = results.iter().find(|r| r.id.contains("node_a")).map(|r| r.similarity).unwrap_or(0.0);
    let score_b = results.iter().find(|r| r.id.contains("node_b")).map(|r| r.similarity).unwrap_or(0.0);
    
    assert!(score_a > score_b, "Node A (newer temporal_range_end) should decay less than Node B (no temporal_range_end): score_a = {}, score_b = {}", score_a, score_b);

    Ok(())
}

#[tokio::test]
async fn test_temporal_neighbor_expansion_retrieves_wiki_nodes() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Enable temporal expansion by configuring temporal cue
    // Insert primary episode Ep1
    let ep = EpisodeSave {
        title: "Ep1".to_string(),
        content: "Event primary context text".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let ep_id = backend.save_episode(&ep).await?;
    
    // Insert neighbor WikiNode Wiki1
    backend.db.query("CREATE type::record('wiki_node', 'wiki1') CONTENT { name: 'Wiki1', content: 'Subsequent details that follow the event', scope: 'general' };").await?.check()?;
    
    // Relate Ep1 -> followed_by -> Wiki1
    backend.relate_followed_by(&ep_id, "wiki_node:wiki1").await?;
    
    // Run search with temporal cue in query to trigger temporal neighbor expansion (e.g. "after the event")
    let search_params = SearchParams {
        query: "after the event".to_string(),
        scope: Some("general".to_string()),
        include_episodes: true,
        limit: 10,
        ..Default::default()
    };
    
    let resp = backend.search(search_params).await?;
    let has_wiki_neighbor = resp.results.iter().any(|r| r.id.contains("wiki1"));
    assert!(has_wiki_neighbor, "Succeeding temporal neighbor expansion must retrieve the linked wiki_node neighbor");

    Ok(())
}
