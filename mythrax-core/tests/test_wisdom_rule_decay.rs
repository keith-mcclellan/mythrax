use std::sync::Arc;
use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::contracts::{WisdomRule, Tier};
use mythrax_core::db::graduation_pipeline::run_graduation_pipeline;

#[tokio::test]
async fn test_wisdom_rule_decay() -> anyhow::Result<()> {
    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;

    let now = chrono::Utc::now();
    let one_year_ago = now - chrono::Duration::days(365);
    let two_years_ago = now - chrono::Duration::days(730);

    let rule_now = WisdomRule {
        id: Some("wisdom:rule_now".to_string()),
        target_pattern: "Pattern Now".to_string(),
        action_to_avoid: "Avoid Now".to_string(),
        causal_explanation: "Cause Now".to_string(),
        prescribed_remedy: "Remedy Now".to_string(),
        tier: Tier::Wisdom,
        scope: "global".to_string(),
        utility: Some(1.0),
        ..Default::default()
    };

    let rule_1yr = WisdomRule {
        id: Some("wisdom:rule_1yr".to_string()),
        target_pattern: "Pattern 1yr".to_string(),
        action_to_avoid: "Avoid 1yr".to_string(),
        causal_explanation: "Cause 1yr".to_string(),
        prescribed_remedy: "Remedy 1yr".to_string(),
        tier: Tier::Wisdom,
        scope: "global".to_string(),
        utility: Some(1.0),
        ..Default::default()
    };

    let rule_2yr = WisdomRule {
        id: Some("wisdom:rule_2yr".to_string()),
        target_pattern: "Pattern 2yr".to_string(),
        action_to_avoid: "Avoid 2yr".to_string(),
        causal_explanation: "Cause 2yr".to_string(),
        prescribed_remedy: "Remedy 2yr".to_string(),
        tier: Tier::Wisdom,
        scope: "global".to_string(),
        utility: Some(1.0),
        ..Default::default()
    };

    // Save them to DB
    let sql = "CREATE type::record('wisdom', $id) CONTENT {
        target_pattern: $target_pattern,
        action_to_avoid: $action_to_avoid,
        causal_explanation: $causal_explanation,
        prescribed_remedy: $prescribed_remedy,
        tier: 'Wisdom',
        scope: 'global',
        generator_name: 'Test',
        source_episodes: [],
        created_at: $created_at
    };";

    backend.db.query(sql)
        .bind(("id", "rule_now"))
        .bind(("target_pattern", rule_now.target_pattern.as_str()))
        .bind(("action_to_avoid", rule_now.action_to_avoid.as_str()))
        .bind(("causal_explanation", rule_now.causal_explanation.as_str()))
        .bind(("prescribed_remedy", rule_now.prescribed_remedy.as_str()))
        .bind(("created_at", now))
        .await?.check()?;

    backend.db.query(sql)
        .bind(("id", "rule_1yr"))
        .bind(("target_pattern", rule_1yr.target_pattern.as_str()))
        .bind(("action_to_avoid", rule_1yr.action_to_avoid.as_str()))
        .bind(("causal_explanation", rule_1yr.causal_explanation.as_str()))
        .bind(("prescribed_remedy", rule_1yr.prescribed_remedy.as_str()))
        .bind(("created_at", one_year_ago))
        .await?.check()?;

    backend.db.query(sql)
        .bind(("id", "rule_2yr"))
        .bind(("target_pattern", rule_2yr.target_pattern.as_str()))
        .bind(("action_to_avoid", rule_2yr.action_to_avoid.as_str()))
        .bind(("causal_explanation", rule_2yr.causal_explanation.as_str()))
        .bind(("prescribed_remedy", rule_2yr.prescribed_remedy.as_str()))
        .bind(("created_at", two_years_ago))
        .await?.check()?;

    // Create metrics entries for each rule
    let sql_metrics = "CREATE type::record('metrics', $met_id) CONTENT {
        target_id: type::record('wisdom', $id),
        utility_score: 1.0,
        access_count: 1,
        last_accessed: time::now()
    };";

    backend.db.query(sql_metrics)
        .bind(("met_id", "met_now"))
        .bind(("id", "rule_now"))
        .await?.check()?;

    backend.db.query(sql_metrics)
        .bind(("met_id", "met_1yr"))
        .bind(("id", "rule_1yr"))
        .await?.check()?;

    backend.db.query(sql_metrics)
        .bind(("met_id", "met_2yr"))
        .bind(("id", "rule_2yr"))
        .await?.check()?;

    // Run the graduation pipeline
    run_graduation_pipeline(backend.as_ref(), "test-scope").await?;

    // Retrieve rules and verify utility values from the metrics table
    let mut resp = backend.db.query("SELECT target_id, utility_score FROM metrics ORDER BY target_id;").await?.check()?;
    let results: Vec<serde_json::Value> = resp.take(0)?;

    let get_util = |id: &str| -> f64 {
        results.iter()
            .find(|val| val["target_id"].as_str().unwrap().contains(id))
            .and_then(|val| val["utility_score"].as_f64())
            .unwrap_or(0.0)
    };

    let util_now = get_util("rule_now");
    let util_1yr = get_util("rule_1yr");
    let util_2yr = get_util("rule_2yr");

    println!("DECAY RESULTS: now={}, 1yr={}, 2yr={}", util_now, util_1yr, util_2yr);

    // Rule 1: 0 days old -> should stay close to 1.0 (e.g. > 0.99)
    assert!(util_now > 0.99, "Rule created now should not decay noticeably (util={})", util_now);

    // Rule 2: 365 days old -> should be close to 0.5 (half-life of 365 days)
    assert!((util_1yr - 0.5).abs() < 0.05, "Rule created 1 year ago should decay to ~0.5 (util={})", util_1yr);

    // Rule 3: 730 days old -> should be close to 0.25 (two half-lives)
    assert!((util_2yr - 0.25).abs() < 0.05, "Rule created 2 years ago should decay to ~0.25 (util={})", util_2yr);

    Ok(())
}
