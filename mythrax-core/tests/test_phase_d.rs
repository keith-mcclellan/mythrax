#![cfg(feature = "bench")]

use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::EpisodeSave;

#[tokio::test]
async fn test_t15_temporal_expansion_pool_size() -> Result<()> {
    unsafe { std::env::set_var("MYTHRAX_SESSION_ISOLATION", "false"); }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let mut primary_ids = Vec::new();
    let mut successor_ids = Vec::new();
    
    for i in 0..10 {
        let ch = (b'a' + i) as char;
        let ep_primary = EpisodeSave {
            title: format!("Match {}", ch.to_uppercase()),
            content: format!("This is primary query match context number {} with unique term QueryMatchWord.", i),
            scope: Some("general".to_string()),
            session_id: Some("test_session".to_string()),
            ..Default::default()
        };
        let id_p = backend.save_episode(&ep_primary).await?;
        primary_ids.push(id_p.clone());

        let ep_successor = EpisodeSave {
            title: format!("Successor {}", ch.to_uppercase()),
            content: format!("This is successor turn linked after primary match context {} with SuccessorWord.", i),
            scope: Some("general".to_string()),
            session_id: Some("test_session".to_string()),
            ..Default::default()
        };
        let id_s = backend.save_episode(&ep_successor).await?;
        successor_ids.push(id_s.clone());

        backend.relate_followed_by(&id_p, &id_s).await?;
    }

    // Now test with pool size = 10
    backend.save_profile_key("search.temporal_expansion_pool_size", "10").await?;
    let resp_10 = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "QueryMatchWord after",
        Some("general"),
        false,
        25,
        0,
        0.0,
        None,
        false,
        true,
        true,
        None,
        true,
        None,
    )).await?;
    
    let successors_10_count = resp_10.results.iter()
        .filter(|r| r.title.starts_with("Successor"))
        .count();
    
    assert_eq!(successors_10_count, 10, "With pool size 10, all 10 successors should be retrieved, found: {}", successors_10_count);

    // Now test with pool size = 2
    backend.save_profile_key("search.temporal_expansion_pool_size", "2").await?;
    let resp_2 = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "QueryMatchWord after",
        Some("general"),
        false,
        25,
        0,
        0.0,
        None,
        false,
        true,
        true,
        None,
        true,
        None,
    )).await?;

    let successors_2_count = resp_2.results.iter()
        .filter(|r| r.title.starts_with("Successor"))
        .count();

    assert_eq!(successors_2_count, 2, "With pool size 2, only 2 successors should be retrieved, found: {}", successors_2_count);

    Ok(())
}

#[tokio::test]
async fn test_t16_cross_session_temporal_expansion() -> Result<()> {
    unsafe { std::env::set_var("MYTHRAX_SESSION_ISOLATION", "true"); }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    backend.save_profile_key("search.temporal_expansion_pool_size", "5").await?;

    // Ingest sequential sessions of same user prefix: user123_1 and user123_2
    let ep1 = EpisodeSave {
        title: "Turn 1".to_string(),
        content: "First turn: We started by setting up the database.".to_string(),
        scope: Some("general".to_string()),
        session_id: Some("user123_1".to_string()),
        ..Default::default()
    };
    backend.save_episodes_batch(&[ep1]).await?;

    let ep2 = EpisodeSave {
        title: "Turn 2".to_string(),
        content: "Second turn: We wrote the tests with SearchTerm.".to_string(),
        scope: Some("general".to_string()),
        session_id: Some("user123_2".to_string()),
        ..Default::default()
    };
    backend.save_episodes_batch(&[ep2]).await?;

    // Search under active session user123_2, query with Preceding cue "before"
    let resp = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "SearchTerm before",
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
    )).await?;

    // The results should contain Turn 1 from user123_1 via expansion
    let found_turn1 = resp.results.iter().any(|r| r.title == "Turn 1" && r.session_id.as_deref() == Some("user123_1"));
    assert!(found_turn1, "Should expand and retrieve Turn 1 from previous session of the same user");

    Ok(())
}
