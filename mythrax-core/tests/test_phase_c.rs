#![cfg(feature = "bench")]

use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::EpisodeSave;

#[tokio::test]
async fn test_t7_session_diversity_promotion() -> Result<()> {
    unsafe { std::env::set_var("MYTHRAX_SESSION_ISOLATION", "false"); }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    backend.save_profile_key("search.bypass_sigmoid_gating", "true").await?;

    // Ingest 1 high similarity Episode for Session A (under-represented) to get it into top-10
    let ep_a_top = EpisodeSave {
        title: "Session A Top Match".to_string(),
        content: "Rust database locks are great. UniqueKeywordA".to_string(),
        scope: Some("general".to_string()),
        session_id: Some("session_a".to_string()),
        ..Default::default()
    };
    backend.save_episode(&ep_a_top).await?;

    // Ingest 4 lower similarity Episodes for Session A (which will fall to the remaining pool)
    for i in 0..4 {
        let ep = EpisodeSave {
            title: format!("Session A Low Match {}", i),
            content: "Rust database locks. MinimalMatchA".to_string(),
            scope: Some("general".to_string()),
            session_id: Some("session_a".to_string()),
            ..Default::default()
        };
        backend.save_episode(&ep).await?;
    }

    // Ingest 40 episodes for Session B (high similarity, will occupy kept pool)
    for i in 0..40 {
        let ep = EpisodeSave {
            title: format!("Session B Match {}", i),
            content: "Rust database locks and transaction management. SessionB".to_string(),
            scope: Some("general".to_string()),
            session_id: Some("session_b".to_string()),
            ..Default::default()
        };
        backend.save_episode(&ep).await?;
    }

    // Ingest 40 episodes for Session C (high similarity, will occupy kept pool)
    for i in 0..40 {
        let ep = EpisodeSave {
            title: format!("Session C Match {}", i),
            content: "Rust database locks and transaction management. SessionC".to_string(),
            scope: Some("general".to_string()),
            session_id: Some("session_c".to_string()),
            ..Default::default()
        };
        backend.save_episode(&ep).await?;
    }

    // Search for "Rust database locks"
    let resp = backend.search(
        "Rust database locks",
        Some("general"),
        false,
        10, // limit
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
    // Assert: under-represented Session A gets promoted to at least 3 turns in top-10
    let session_a_count = results.iter().take(10).filter(|r| r.session_id.as_deref() == Some("session_a")).count();
    assert!(session_a_count >= 3, "Session A should be promoted to at least 3 turns in top-10, found: {}", session_a_count);

    Ok(())
}
