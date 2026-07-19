use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::WikiNode;
use chrono::{Utc, Duration};

#[tokio::test]
async fn test_temporal_decay_uses_range_end() -> Result<()> {
    unsafe { std::env::set_var("MYTHRAX_MOCK_LLM", "true"); }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    backend.save_profile_key("search.enable_access_reinforcement", "false").await?;
    backend.save_profile_key("search.enable_gaussian_temporal", "true").await?;
    backend.save_profile_key("search.gaussian_temporal_sigma", "168.0").await?;

    let now = Utc::now();
    let old_created_at = now - Duration::days(30);
    let range_end = now - Duration::days(7); // 1 sigma

    let node = WikiNode {
        id: None,
        name: "Test Node Range End".to_string(),
        content: "Testing temporal range end vs created at".to_string(),
        scope: "general".to_string(),
        temporal_range_end: Some(range_end),
        ..Default::default()
    };
    let id = backend.save_wiki_node(&node).await?;
    let uuid = id.split(':').nth(1).unwrap();

    // override created_at to be much older
    backend.db.query("UPDATE type::record('wiki_node', $id) MERGE { created_at: type::datetime($created_at), utility: 100.0 };")
        .bind(("id", uuid))
        .bind(("created_at", old_created_at.to_rfc3339()))
        .await?.check()?;

    let resp = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "Testing temporal range end",
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

    let matched = resp.results.iter().find(|r| r.id == id).expect("Should find the node");
    
    // Gaussian decay factor for 7 days (168 hours) is exp(-0.5) = 0.60653
    // Decayed utility = 100.0 * 0.60653 = ~60.65
    // If it used created_at (30 days), it would be MUCH lower.
    println!("DEBUG: gaussian utility = {}", matched.utility);
    assert!(matched.utility > 0.5, "Utility should use temporal_range_end (7 days ago), not created_at (30 days ago).");

    Ok(())
}
