#![cfg(feature = "bench")]

use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::db::backend::QueryCategory;
use mythrax_core::contracts::{EpisodeSave, WisdomRule};
use mythrax_core::db::backend::split_temporal_query;

#[test]
fn test_t4_temporal_word_split() {
    let query = "Which book did I finish a week ago?";
    let (fts_query, vector_query) = split_temporal_query(query);
    assert!(vector_query.contains("week") && vector_query.contains("ago"), "vector query must contain temporal words: {}", vector_query);
    assert!(!fts_query.contains("week") && !fts_query.contains("ago"), "fts query must not contain temporal words: {}", fts_query);
}

#[tokio::test]
async fn test_t5_fusion_no_sigmoid_in_pipeline() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Enable sigmoid bypass
    backend.save_profile_key("search.bypass_sigmoid_gating", "true").await?;

    let ep_a = EpisodeSave {
        title: "High Similarity Old Node".to_string(),
        content: "Rust database locks and transaction management.".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let id_a = backend.save_episode(&ep_a).await?;
    let uuid_a = id_a.split(':').nth(1).unwrap();

    // Set importance to 2.0 and created 10 days ago
    backend.db.query("UPDATE type::record('episode', $id) MERGE { importance: 2.0, created_at: time::now() - 10d };")
        .bind(("id", uuid_a))
        .await?.check()?;

    let ep_b = EpisodeSave {
        title: "Low Similarity Recent Node".to_string(),
        content: "Completely unrelated text about cooking recipes and kitchen tools.".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let id_b = backend.save_episode(&ep_b).await?;
    let uuid_b = id_b.split(':').nth(1).unwrap();

    // Set importance to 10.0 and created 0 days ago
    backend.db.query("UPDATE type::record('episode', $id) MERGE { importance: 10.0, created_at: time::now() };")
        .bind(("id", uuid_b))
        .await?.check()?;

    // Search
    let resp = backend.search(
        "Rust database locks",
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
        None,
    ).await?;

    let results = resp.results;
    assert!(!results.is_empty());
    
    // With bypass, scores should not be squashed by sigmoids, so let's verify score_b is > 0.75 (unlike with sigmoids)
    if let Some(pos_b) = results.iter().position(|r| r.id == id_b) {
        let score_b = results[pos_b].similarity;
        assert!(score_b > 0.75, "Low similarity node score should not be suppressed under bypass: {}", score_b);
    }

    Ok(())
}

#[tokio::test]
async fn test_t8_factor_multiplier_single_application() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Enable sigmoid bypass
    backend.save_profile_key("search.bypass_sigmoid_gating", "true").await?;

    let ep = EpisodeSave {
        title: "Database Lock".to_string(),
        content: "Rust database locks and transaction management.".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let id = backend.save_episode(&ep).await?;
    let uuid = id.split(':').nth(1).unwrap();

    // High importance: 8.0 (default is 1.0)
    backend.db.query("UPDATE type::record('episode', $id) MERGE { importance: 8.0, created_at: time::now() };")
        .bind(("id", uuid))
        .await?.check()?;

    let resp = backend.search(
        "Rust database locks",
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
        None,
    ).await?;

    let results = resp.results;
    assert!(!results.is_empty());
    let score = results[0].similarity;
    
    // Factor multiplier should be single-applied. Under double-application, it would be extremely high or squared.
    // Verify it is single-applied (ratio of score with importance 8.0 vs 1.0 is single-applied, so total score < 3.0)
    assert!(score < 3.0, "Score under single factor application should be reasonable (< 3.0), found: {}", score);

    Ok(())
}

#[tokio::test]
async fn test_t12_default_category_no_aggressive_decay() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Enable sigmoid bypass and disable ladder scale/decay floor for default category to align FTS scores and allow deep decay
    backend.save_profile_key("search.bypass_sigmoid_gating", "true").await?;
    backend.save_profile_key("search.default.ladder_scale", "0.000").await?;
    backend.save_profile_key("search.temporal_decay_floor", "0.00").await?;

    // 10-day-old episode
    let ep_old = EpisodeSave {
        title: "Episode".to_string(),
        content: "Some unique database locking content here.".to_string(),
        scope: Some("general".to_string()),
        created_at: Some("2023-05-20T23:40:00Z".to_string()),
        ..Default::default()
    };
    let id_old = backend.save_episode(&ep_old).await?;
    let uuid_old = id_old.split(':').nth(1).unwrap();
    backend.db.query("UPDATE type::record('episode', $id) MERGE { importance: 1.0, last_retrieved_at: NONE };")
        .bind(("id", uuid_old))
        .await?.check()?;

    // Fresh episode
    let ep_fresh = EpisodeSave {
        title: "Episode".to_string(),
        content: "Some unique database locking content here.".to_string(),
        scope: Some("general".to_string()),
        created_at: Some("2023-05-30T23:40:00Z".to_string()),
        ..Default::default()
    };
    let id_fresh = backend.save_episode(&ep_fresh).await?;
    let uuid_fresh = id_fresh.split(':').nth(1).unwrap();
    backend.db.query("UPDATE type::record('episode', $id) MERGE { importance: 1.0, last_retrieved_at: NONE };")
        .bind(("id", uuid_fresh))
        .await?.check()?;

    // Search with Default category query (e.g. "what is the weather in Tokyo")
    // Wait, we search for "database locking" to retrieve both. But we want to ensure the classification is Default.
    // So we can make the query classification Default. "what is database locking" classifies as Default.
    let resp = backend.search(
        "what is database locking",
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
        Some("2023-05-30T23:40:00Z"),
    ).await?;

    let results = resp.results;
    assert!(results.len() >= 2);
    let r_old = results.iter().find(|r| r.id == id_old).expect("Old episode not found");
    let r_fresh = results.iter().find(|r| r.id == id_fresh).expect("Fresh episode not found");


    let ratio = r_old.factor_multiplier.unwrap() / r_fresh.factor_multiplier.unwrap();
    // With sigma = 168h (7 days), at 10 days decay factor is strictly between 0.25 and 0.50
    assert!(ratio >= 0.25 && ratio <= 0.50, "Ratio should be between 0.25 and 0.50, found: {}", ratio);

    // Ingest 30-day-old episode
    let ep_very_old = EpisodeSave {
        title: "Episode".to_string(),
        content: "Some unique database locking content here.".to_string(),
        scope: Some("general".to_string()),
        created_at: Some("2023-04-30T23:40:00Z".to_string()),
        ..Default::default()
    };
    let id_very_old = backend.save_episode(&ep_very_old).await?;
    let uuid_very_old = id_very_old.split(':').nth(1).unwrap();
    backend.db.query("UPDATE type::record('episode', $id) MERGE { importance: 1.0, last_retrieved_at: NONE };")
        .bind(("id", uuid_very_old))
        .await?.check()?;

    let resp2 = backend.search(
        "what is database locking",
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
        Some("2023-05-30T23:40:00Z"),
    ).await?;

    let r_very_old = resp2.results.iter().find(|r| r.id == id_very_old).expect("Very old episode not found");
    let r_fresh2 = resp2.results.iter().find(|r| r.id == id_fresh).expect("Fresh episode not found");
    let ratio_very_old = r_very_old.factor_multiplier.unwrap() / r_fresh2.factor_multiplier.unwrap();
    // 30 days decays to < 0.10
    assert!(ratio_very_old < 0.10, "Ratio for very old episode must be < 0.10, found: {}", ratio_very_old);

    Ok(())
}

#[tokio::test]
async fn test_t13_bm25_outlier_stability() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Enable sigmoid bypass
    backend.save_profile_key("search.bypass_sigmoid_gating", "true").await?;

    // Ingest Episode A: Extreme BM25 match
    let ep_a = EpisodeSave {
        title: "Extreme Match".to_string(),
        content: "Rust database locks Rust database locks Rust database locks Rust database locks".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let id_a = backend.save_episode(&ep_a).await?;

    // Ingest Episode B: Moderate match
    let ep_b = EpisodeSave {
        title: "Moderate Match".to_string(),
        content: "Rust database locks".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let id_b = backend.save_episode(&ep_b).await?;

    let resp = backend.search(
        "Rust database locks",
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
        None,
    ).await?;

    let results = resp.results;
    assert!(results.iter().any(|r| r.id == id_a));
    assert!(results.iter().any(|r| r.id == id_b), "Moderate candidate should still be retrieved and ranked");

    Ok(())
}

#[tokio::test]
async fn test_t14_tier_boost_after_factor_fix() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Enable sigmoid bypass and disable gamma rerank + calibrated confidence to isolate tier boost factor
    backend.save_profile_key("search.bypass_sigmoid_gating", "true").await?;
    backend.save_profile_key("search.gamma_rerank", "0.0").await?;
    backend.save_profile_key("search.enable_calibrated_confidence", "false").await?;

    // Save an episode (Default category: factor = (0.3*0.5 + 0.3*1.0)/0.6 * 1.0 = 0.75)
    let ep = EpisodeSave {
        title: "Episode Node".to_string(),
        content: "Database locking mechanisms are important".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let id_ep = backend.save_episode(&ep).await?;

    // Save a wisdom rule (Default category: factor = (0.5*0.5 + 0.1*1.0)/0.6 * 1.2 = 0.7)
    let rule = WisdomRule {
        target_pattern: "Wiki Node".to_string(),
        action_to_avoid: "database locking conflicts".to_string(),
        causal_explanation: "concurrent access".to_string(),
        prescribed_remedy: "Use client mode".to_string(),
        tier: "skills".to_string(),
        scope: "general".to_string(),
        generator_name: "manual".to_string(),
        ..Default::default()
    };
    let id_r = backend.save_wisdom_rule(&rule).await?;

    let resp = backend.search(
        "database locking",
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
        None,
    ).await?;

    let results = resp.results;

    let ep_result = results.iter().find(|r| r.id == id_ep).expect("Episode not found");
    let wis_result = results.iter().find(|r| r.id == id_r).expect("Wisdom rule not found");

    // Verify the factor_multiplier ordering: episode should have higher factor
    let ep_factor = ep_result.factor_multiplier.unwrap();
    let wis_factor = wis_result.factor_multiplier.unwrap();
    assert!(ep_factor > wis_factor,
        "Episode factor_multiplier ({}) must be > wisdom factor_multiplier ({})",
        ep_factor, wis_factor);

    // With confounding factors disabled, the higher factor_multiplier should produce higher similarity
    assert!(ep_result.similarity > wis_result.similarity,
        "Episode similarity ({}) must be > wisdom similarity ({}) due to higher factor_multiplier",
        ep_result.similarity, wis_result.similarity);

    Ok(())
}
