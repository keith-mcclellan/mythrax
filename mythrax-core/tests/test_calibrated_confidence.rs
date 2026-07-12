use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::EpisodeSave;

#[tokio::test]
async fn test_calibrated_confidence_scaling() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create a mock episode with confidence 0.50
    let ep = EpisodeSave {
        created_at: None,
        title: "Calibrated Confidence Test Episode".to_string(),
        content: "Unique content for test scaling similarity".to_string(),
        scope: Some("general".to_string()),
        confidence: Some(0.50),
        ..Default::default()
    };

    let id = backend.save_episode(&ep).await?;
    let uuid = id.split(':').nth(1).unwrap();

    // Ensure it is not archived and has confidence set
    backend.db.query("UPDATE type::record('episode', $id) MERGE { confidence: 0.50, archived: false };")
        .bind(("id", uuid))
        .await?.check()?;

    // 1. Enable calibrated confidence
    backend.save_profile_key("search.enable_calibrated_confidence", "true").await?;

    let resp = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "Unique content for test scaling similarity",
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
    assert_eq!(matched.confidence, Some(0.50), "Confidence must be populated as Some(0.50)");

    let scaled_similarity = matched.similarity;

    // 2. Disable calibrated confidence
    backend.save_profile_key("search.enable_calibrated_confidence", "false").await?;

    let resp_disabled = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "Unique content for test scaling similarity",
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

    let results_disabled = resp_disabled.results;
    let matched_disabled = results_disabled.iter().find(|r| r.id == id).expect("Should find the exact episode");
    let unscaled_similarity = matched_disabled.similarity;

    println!("DEBUG: scaled = {}, unscaled = {}", scaled_similarity, unscaled_similarity);
    // Since confidence is 0.50, scaled similarity should be exactly half of the unscaled one.
    assert!((scaled_similarity - unscaled_similarity * 0.50).abs() < 1e-4, "Scaled similarity must be exactly confidence (0.50) times unscaled similarity");

    Ok(())
}
