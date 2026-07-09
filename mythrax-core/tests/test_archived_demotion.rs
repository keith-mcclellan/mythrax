use anyhow::Result;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};

#[tokio::test]
async fn test_archived_demotion_logic() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    backend
        .save_profile_key("search.sigmoid_center", "0.55")
        .await?;
    backend
        .save_profile_key("search.fusion_sigmoid_center", "0.60")
        .await?;
    backend
        .save_profile_key("search.gamma_rerank", "0.10")
        .await?;
    backend
        .save_profile_key("search.rerank_pool_size", "25")
        .await?;
    backend
        .save_profile_key("search.rerank_weight", "0.45")
        .await?;

    // Create 7 episodes with the same content so their raw vector similarity is identical
    let content = "Database index compression algorithms and HNSW graphs.";
    let titles = vec![
        "Active Node",                 // Index 0: Not archived
        "Recent Archived Node",        // Index 1: Archived 30m ago (factor 0.85)
        "One Day Archived Node",       // Index 2: Archived 24h ago (factor 0.85)
        "Three Days Archived Node",    // Index 3: Archived 3d ago (factor 0.70)
        "Seven Days Archived Node",    // Index 4: Archived 7d ago (factor 0.40)
        "Fourteen Days Archived Node", // Index 5: Archived 14d ago (factor 0.40)
        "Legacy Archived Node",        // Index 6: Archived, archived_at is None (factor 0.40)
    ];

    let mut ids = Vec::new();
    for (i, title) in titles.iter().enumerate() {
        let session_id = if i == 0 || i == 1 {
            Some("session-123".to_string())
        } else {
            Some("session-abc".to_string())
        };
        let ep = EpisodeSave {
            title: title.to_string(),
            content: content.to_string(),
            scope: Some("general".to_string()),
            session_id,
            ..Default::default()
        };
        let id = backend.save_episode(&ep).await?;
        ids.push(id);
    }

    // Set archived and archived_at fields for each node
    // 0: Active (archived: false, archived_at: None - defaults)

    // 1: Recent (archived: true, archived_at: now - 30m)
    let uuid_1 = ids[1].split(':').nth(1).unwrap();
    backend.db.query("UPDATE type::record('episode', $id) MERGE { archived: true, archived_at: time::now() - 30m };")
        .bind(("id", uuid_1))
        .await?.check()?;

    // 2: One Day (archived: true, archived_at: now - 24h)
    let uuid_2 = ids[2].split(':').nth(1).unwrap();
    backend.db.query("UPDATE type::record('episode', $id) MERGE { archived: true, archived_at: time::now() - 24h };")
        .bind(("id", uuid_2))
        .await?.check()?;

    // 3: Three Days (archived: true, archived_at: now - 3d)
    let uuid_3 = ids[3].split(':').nth(1).unwrap();
    backend.db.query("UPDATE type::record('episode', $id) MERGE { archived: true, archived_at: time::now() - 3d };")
        .bind(("id", uuid_3))
        .await?.check()?;

    // 4: Seven Days (archived: true, archived_at: now - 7d)
    let uuid_4 = ids[4].split(':').nth(1).unwrap();
    backend.db.query("UPDATE type::record('episode', $id) MERGE { archived: true, archived_at: time::now() - 7d };")
        .bind(("id", uuid_4))
        .await?.check()?;

    // 5: Fourteen Days (archived: true, archived_at: now - 14d)
    let uuid_5 = ids[5].split(':').nth(1).unwrap();
    backend.db.query("UPDATE type::record('episode', $id) MERGE { archived: true, archived_at: time::now() - 14d };")
        .bind(("id", uuid_5))
        .await?.check()?;

    // 6: Legacy (archived: true, archived_at: None)
    let uuid_6 = ids[6].split(':').nth(1).unwrap();
    backend
        .db
        .query("UPDATE type::record('episode', $id) MERGE { archived: true, archived_at: None };")
        .bind(("id", uuid_6))
        .await?
        .check()?;

    // Allow SurrealDB FTS to index
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    // Debug print all episodes in the database
    let mut all_eps_res = backend
        .db
        .query("SELECT id, title, session_id, archived, archived_at FROM episode;")
        .await?;
    let all_eps: Vec<serde_json::Value> = all_eps_res.take(0)?;
    println!("ALL EPISODES IN DB: {:#?}", all_eps);

    // 1. CROSS-SESSION SEARCH: Search with session_id = None (meaning all retrieved nodes from different sessions are cross-session)
    unsafe {
        std::env::set_var("MYTHRAX_SESSION_ISOLATION", "false");
    }
    let resp_cross = backend
        .search(
            "Database index compression",
            Some("general"),
            false,
            20,
            0,
            0.0,
            None,
            false,
            true,
            true,
            None,
            true,
        )
        .await?;
    unsafe {
        std::env::set_var("MYTHRAX_SESSION_ISOLATION", "true");
    }

    let results_cross = resp_cross.results;
    println!(
        "CROSS-SESSION RESULTS: {:?}",
        results_cross.iter().map(|r| &r.title).collect::<Vec<_>>()
    );
    assert_eq!(
        results_cross.len(),
        7,
        "Should return all 7 search results for cross-session search"
    );

    let get_score_cross = |id: &str| -> f32 {
        results_cross
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.similarity)
            .unwrap_or(0.0)
    };

    let score_active_cross = get_score_cross(&ids[0]);
    let score_recent_cross = get_score_cross(&ids[1]);
    let score_one_day_cross = get_score_cross(&ids[2]);
    let score_three_days_cross = get_score_cross(&ids[3]);
    let score_seven_days_cross = get_score_cross(&ids[4]);
    let score_fourteen_days_cross = get_score_cross(&ids[5]);
    let score_legacy_cross = get_score_cross(&ids[6]);

    println!("CROSS-SESSION Active Score: {}", score_active_cross);
    println!(
        "CROSS-SESSION Recent Score (expect demoted, ratio ~0.4): {}",
        score_recent_cross
    );
    println!(
        "CROSS-SESSION One Day Score (expect demoted, ratio ~0.4): {}",
        score_one_day_cross
    );

    // Verify cross-session ratios (all archived nodes should be demoted by 0.4)
    let ratio_recent_cross = score_recent_cross / score_active_cross;
    assert!(
        (ratio_recent_cross - 0.40).abs() < 0.05,
        "Recent cross-session ratio was {}",
        ratio_recent_cross
    );

    let ratio_one_day_cross = score_one_day_cross / score_active_cross;
    assert!(
        (ratio_one_day_cross - 0.40).abs() < 0.05,
        "One day cross-session ratio was {}",
        ratio_one_day_cross
    );

    let ratio_three_days_cross = score_three_days_cross / score_active_cross;
    assert!(
        (ratio_three_days_cross - 0.40).abs() < 0.05,
        "Three days cross-session ratio was {}",
        ratio_three_days_cross
    );

    let ratio_seven_days_cross = score_seven_days_cross / score_active_cross;
    assert!(
        (ratio_seven_days_cross - 0.40).abs() < 0.05,
        "Seven days cross-session ratio was {}",
        ratio_seven_days_cross
    );

    let ratio_fourteen_days_cross = score_fourteen_days_cross / score_active_cross;
    assert!(
        (ratio_fourteen_days_cross - 0.40).abs() < 0.05,
        "Fourteen days cross-session ratio was {}",
        ratio_fourteen_days_cross
    );

    let ratio_legacy_cross = score_legacy_cross / score_active_cross;
    assert!(
        (ratio_legacy_cross - 0.40).abs() < 0.05,
        "Legacy cross-session ratio was {}",
        ratio_legacy_cross
    );

    // 2. SAME-SESSION SEARCH: Search with session_id = Some("session-123")
    // This will retrieve only Index 0 (Active) and Index 1 (Recent Archived), as they are same-session.
    let resp_same = backend
        .search(
            "Database index compression",
            Some("general"),
            false,
            20,
            0,
            0.0,
            None,
            false,
            true,
            true,
            Some("session-123"),
            true,
        )
        .await?;

    let results_same = resp_same.results;
    println!(
        "SAME-SESSION RESULTS: {:?}",
        results_same.iter().map(|r| &r.title).collect::<Vec<_>>()
    );
    assert_eq!(
        results_same.len(),
        2,
        "Should return 2 search results matching the session"
    );

    let get_score_same = |id: &str| -> f32 {
        results_same
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.similarity)
            .unwrap_or(0.0)
    };

    let score_active_same = get_score_same(&ids[0]);
    let score_recent_same = get_score_same(&ids[1]);

    println!("SAME-SESSION Active Score: {}", score_active_same);
    println!(
        "SAME-SESSION Recent Score (expect same-session bypass, ratio ~1.0): {}",
        score_recent_same
    );

    // Verify same-session ratio (should bypass demotion, so ratio is ~1.0)
    let ratio_recent_same = score_recent_same / score_active_same;
    assert!(
        (ratio_recent_same - 1.0).abs() < 0.05,
        "Recent same-session ratio was {}",
        ratio_recent_same
    );

    Ok(())
}
