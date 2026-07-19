use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::EpisodeSave;

#[tokio::test]
async fn test_proactive_pruned_injection() -> Result<()> {
    unsafe { std::env::set_var("MYTHRAX_MOCK_LLM", "true"); }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    
    // Test that pruned/archived nodes are not proactively injected in normal queries
    let ep = EpisodeSave {
        title: "Pruned Episode".to_string(),
        content: "This episode is pruned".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let ep_id = backend.save_episode(&ep).await?;
    
    // Archive it
    let uuid = ep_id.split(':').nth(1).unwrap();
    backend.db.query("UPDATE type::record('episode', $id) MERGE { archived: true };")
        .bind(("id", uuid))
        .await?.check()?;

    let resp = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "This episode is pruned",
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

    let found = resp.results.iter().any(|r| r.id == ep_id);
    assert!(!found, "Archived/pruned episodes should not be proactively injected into results");

    Ok(())
}
