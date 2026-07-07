use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::{EpisodeSave, WisdomRule};

#[tokio::test]
async fn test_sigmoid_gated_retrieval_formula() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_SIGMOID_GATED_SEARCH_TEST", "true");
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // 1. Insert Mock Episode A: High similarity (0.85), low importance (2.0), old (created 10 days ago)
    let ep_a = EpisodeSave {
        title: "High Similarity Old Node".to_string(),
        content: "Rust database locks and transaction management.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("episodes/ep_a.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        ..Default::default()
    };
    let id_a = backend.save_episode(&ep_a).await?;
    let uuid_a = id_a.split(':').nth(1).unwrap();

    // Set importance to 2.0 and simulate creation 10 days ago
    backend.db.query("UPDATE type::record('episode', $id) MERGE { importance: 2.0, created_at: time::now() - 10d };")
        .bind(("id", uuid_a))
        .await?.check()?;

    // 2. Insert Mock Episode B: Low similarity (0.50), high importance (10.0), extremely recent (0 days ago)
    let ep_b = EpisodeSave {
        title: "Low Similarity Recent Node".to_string(),
        content: "Completely unrelated text about cooking recipes and kitchen tools.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("episodes/ep_b.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        ..Default::default()
    };
    let id_b = backend.save_episode(&ep_b).await?;
    let uuid_b = id_b.split(':').nth(1).unwrap();

    backend.db.query("UPDATE type::record('episode', $id) MERGE { importance: 10.0, created_at: time::now() };")
        .bind(("id", uuid_b))
        .await?.check()?;

    // 3. Search for "Rust database locks"
    let resp = backend.search("Rust database locks",  Some("general"),  false,  10,  0,  0.0,  None,  false,  true,  true, None, true).await?;
    
    // Assertions
    let results = resp.results;
    assert!(!results.is_empty(), "Should return search results");

    let pos_a = results.iter().position(|r| r.id == id_a);
    let pos_b = results.iter().position(|r| r.id == id_b);

    assert!(pos_a.is_some(), "High similarity node must be retrieved");
    if let Some(pb) = pos_b {
        assert!(pos_a.unwrap() < pb, "High similarity node must rank higher than gated low similarity node");
        let score_b = results[pb].similarity;
        println!("DEBUG: score_b = {}", score_b);
        assert!(score_b <= 0.75, "Low similarity node score must be heavily suppressed by the sigmoid gate");
    }

    // 4. Verify Wisdom Rule decay immunity
    let rule = WisdomRule {
        id: None,
        target_pattern: "avoid_concurrency".to_string(),
        action_to_avoid: "Writing concurrently".to_string(),
        causal_explanation: "RocksDB process lock".to_string(),
        prescribed_remedy: "Use client mode".to_string(),
        tier: "skills".to_string(),
        scope: "general".to_string(),
        vault_path: Some("wisdom/skills/avoid_concurrency.md".to_string()),
        embedding: None,
        source_episodes: vec![],
        generator_name: "manual".to_string(),
        similarity: None,
        utility: Some(50.0),
        status: Some("active".to_string()),
        superseded_at: None,
        superseded_by: None,
    
        rule_type: None,};
    let id_r = backend.save_wisdom_rule(&rule).await?;
    let uuid_r = id_r.split(':').nth(1).unwrap();

    // Simulate creation 30 days ago
    backend.db.query("UPDATE type::record('wisdom', $id) MERGE { importance: 8.0, created_at: time::now() - 30d };")
        .bind(("id", uuid_r))
        .await?.check()?;

    // Search for wisdom
    let resp_r = backend.search("avoid_concurrency",  Some("general"),  false,  10,  0,  0.0,  None,  false,  true,  true, None, true).await?;
    let r_results = resp_r.results;
    let match_rule = r_results.iter().find(|r| r.id == id_r);
    assert!(match_rule.is_some(), "Wisdom rule must be retrieved despite being 30 days old due to decay immunity");

    Ok(())
}
