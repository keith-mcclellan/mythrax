use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::EpisodeSave;
use std::sync::Arc;

#[tokio::test]
async fn test_archived_demotion_logic() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    backend.save_profile_key("search.ladder_scale", "0.0").await?;

    // Create 7 episodes with the same content so their raw vector similarity is identical
    let content = "Database index compression algorithms and HNSW graphs.";
    let titles = vec![
        "Active Node",                     // Index 0: Not archived
        "Recent Archived Node",            // Index 1: Archived 30m ago (factor 0.85)
        "One Day Archived Node",           // Index 2: Archived 24h ago (factor 0.85)
        "Three Days Archived Node",        // Index 3: Archived 3d ago (factor 0.70)
        "Seven Days Archived Node",        // Index 4: Archived 7d ago (factor 0.40)
        "Fourteen Days Archived Node",     // Index 5: Archived 14d ago (factor 0.40)
        "Legacy Archived Node",            // Index 6: Archived, archived_at is None (factor 0.40)
    ];

    let mut ids = Vec::new();
    for title in &titles {
        let ep = EpisodeSave {
            title: title.to_string(),
            content: content.to_string(),
            scope: Some("general".to_string()),
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
    backend.db.query("UPDATE type::record('episode', $id) MERGE { archived: true, archived_at: None };")
        .bind(("id", uuid_6))
        .await?.check()?;

    // Allow SurrealDB FTS to index
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    // Search for "Database index compression"
    let resp = backend.search(
        "Database index compression",
        Some("general"),
        false,
        20, // Fetch all 7
        0,
        0.0,
        None,
        false,
        true,
        true,
    ).await?;

    let results = resp.results;
    assert_eq!(results.len(), 7, "Should return all 7 search results");

    // Extract scores for each node
    let get_score = |id: &str| -> f32 {
        results.iter().find(|r| r.id == id).map(|r| r.similarity).unwrap_or(0.0)
    };

    let score_active = get_score(&ids[0]);
    let score_recent = get_score(&ids[1]);
    let score_one_day = get_score(&ids[2]);
    let score_three_days = get_score(&ids[3]);
    let score_seven_days = get_score(&ids[4]);
    let score_fourteen_days = get_score(&ids[5]);
    let score_legacy = get_score(&ids[6]);

    println!("Active Score: {}", score_active);
    println!("Recent Score (expect ~0.85x active): {}", score_recent);
    println!("One Day Score (expect ~0.85x active): {}", score_one_day);
    println!("Three Days Score (expect ~0.70x active): {}", score_three_days);
    println!("Seven Days Score (expect ~0.40x active): {}", score_seven_days);
    println!("Fourteen Days Score (expect ~0.40x active): {}", score_fourteen_days);
    println!("Legacy Score (expect ~0.40x active): {}", score_legacy);

    // Verify ratios
    // 1. Recent (30m) factor should be ~0.85 of active
    let ratio_recent = score_recent / score_active;
    assert!((ratio_recent - 0.85).abs() < 0.05, "Recent ratio was {}", ratio_recent);

    // 2. One Day factor should be ~0.85 of active
    let ratio_one_day = score_one_day / score_active;
    assert!((ratio_one_day - 0.85).abs() < 0.05, "One day ratio was {}", ratio_one_day);

    // 3. Three Days factor should be ~0.70 of active
    let ratio_three_days = score_three_days / score_active;
    assert!((ratio_three_days - 0.70).abs() < 0.05, "Three days ratio was {}", ratio_three_days);

    // 4. Seven Days factor should be ~0.40 of active
    let ratio_seven_days = score_seven_days / score_active;
    assert!((ratio_seven_days - 0.40).abs() < 0.05, "Seven days ratio was {}", ratio_seven_days);

    // 5. Fourteen Days factor should be ~0.40 of active
    let ratio_fourteen_days = score_fourteen_days / score_active;
    assert!((ratio_fourteen_days - 0.40).abs() < 0.05, "Fourteen days ratio was {}", ratio_fourteen_days);

    // 6. Legacy (None) factor should be ~0.40 of active (fallback works)
    let ratio_legacy = score_legacy / score_active;
    assert!((ratio_legacy - 0.40).abs() < 0.05, "Legacy ratio was {}", ratio_legacy);

    Ok(())
}
