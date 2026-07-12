use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::EpisodeSave;

#[tokio::test]
async fn test_gaussian_temporal_decay() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    backend.save_profile_key("search.enable_access_reinforcement", "false").await?;

    // Create a mock episode with utility 100.0
    let ep = EpisodeSave {
        created_at: None,
        title: "Gaussian Temporal Test Episode".to_string(),
        content: "Unique content for test gaussian temporal decay".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };

    let id = backend.save_episode(&ep).await?;
    let uuid = id.split(':').nth(1).unwrap();

    // Set utility to 100.0 and simulate last_retrieved_at and created_at to 7 days ago (7 days * 24 hours = 168 hours = 1 sigma)
    backend.db.query("UPDATE type::record('episode', $id) MERGE { utility: 100.0, created_at: time::now() - 7d, last_retrieved_at: time::format(time::now() - 7d, '%Y-%m-%dT%H:%M:%SZ'), archived: false };")
        .bind(("id", uuid))
        .await?.check()?;

    // 1. Enable Gaussian temporal decay
    backend.save_profile_key("search.enable_gaussian_temporal", "true").await?;
    backend.save_profile_key("search.gaussian_temporal_sigma", "168.0").await?;

    let resp = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "Unique content for test gaussian temporal decay",
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

    let results = resp.results;
    assert!(!results.is_empty(), "Should retrieve the episode");
    let matched = results.iter().find(|r| r.id == id).expect("Should find the exact episode");
    
    // Gaussian decay factor for 7 days (168 hours) is exp(-0.5) = 0.60653
    // Decayed utility = 100.0 * 0.60653 = 60.653
    let gaussian_utility = matched.utility;
    println!("DEBUG: gaussian utility = {}", gaussian_utility);

    // 2. Disable Gaussian temporal decay (fallback to linear/exponential with -0.05 * delta_t_days)
    backend.save_profile_key("search.enable_gaussian_temporal", "false").await?;

    let resp_fallback = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "Unique content for test gaussian temporal decay",
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

    let results_fallback = resp_fallback.results;
    let matched_fallback = results_fallback.iter().find(|r| r.id == id).expect("Should find the exact episode");

    // Standard decay factor for 7 days is exp(-0.05 * 7) = exp(-0.35) = 0.704688
    // Decayed utility = 100.0 * 0.704688 = 70.4688
    let standard_utility = matched_fallback.utility;
    println!("DEBUG: standard utility = {}", standard_utility);

    // Assert that the utility scores match the theoretical decay factors
    assert!((gaussian_utility - 60.653).abs() < 1.0, "Gaussian utility should be around 60.65");
    assert!((standard_utility - 70.47).abs() < 1.0, "Standard utility should be around 70.47");

    Ok(())
}
