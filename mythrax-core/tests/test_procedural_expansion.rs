use anyhow::Result;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend, parse_record_id};

#[tokio::test]
async fn test_procedural_cue_neighbor_expansion() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create 4 sequential episodes
    let ep0 = EpisodeSave {
        title: "Step 0 CLI Start".to_string(),
        content: "First command line interface run. Initialized repo successfully.".to_string(),
        scope: Some("general".to_string()),
        session_id: Some("session-procedural".to_string()),
        ..Default::default()
    };
    let ep0_id = backend.save_episode(&ep0).await?;

    let ep1 = EpisodeSave {
        title: "Step 1 Compile Action".to_string(),
        content: "Second compile action. Compiling main module now.".to_string(),
        scope: Some("general".to_string()),
        session_id: Some("session-procedural".to_string()),
        ..Default::default()
    };
    let ep1_id = backend.save_episode(&ep1).await?;

    let ep2 = EpisodeSave {
        title: "Step 2 Dev Deploy".to_string(),
        content: "Third deployment script step. Deploying dev server.".to_string(),
        scope: Some("general".to_string()),
        session_id: Some("session-procedural".to_string()),
        ..Default::default()
    };
    let ep2_id = backend.save_episode(&ep2).await?;

    let ep3 = EpisodeSave {
        title: "Step 3 Health Verify".to_string(),
        content: "Fourth verification curl. Checked localhost health status page.".to_string(),
        scope: Some("general".to_string()),
        session_id: Some("session-procedural".to_string()),
        ..Default::default()
    };
    let ep3_id = backend.save_episode(&ep3).await?;

    // Link: ep0 -> followed_by -> ep1 -> followed_by -> ep2 -> followed_by -> ep3
    let rec0 = parse_record_id(&ep0_id)?;
    let rec1 = parse_record_id(&ep1_id)?;
    let rec2 = parse_record_id(&ep2_id)?;
    let rec3 = parse_record_id(&ep3_id)?;

    backend
        .db
        .query("RELATE $from -> followed_by -> $to;")
        .bind(("from", rec0.clone()))
        .bind(("to", rec1.clone()))
        .await?
        .check()?;

    backend
        .db
        .query("RELATE $from -> followed_by -> $to;")
        .bind(("from", rec1.clone()))
        .bind(("to", rec2.clone()))
        .await?
        .check()?;

    backend
        .db
        .query("RELATE $from -> followed_by -> $to;")
        .bind(("from", rec2.clone()))
        .bind(("to", rec3.clone()))
        .await?
        .check()?;

    // Allow SurrealDB index
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    // Search query matches "Second compile action" (ep1)
    // Query is a procedural question which triggers depth=3 bidirectional expansion
    let query = "What compile actions did I run?";

    let resp = backend
        .search(
            query,
            Some("general"),
            true,
            // deep_insight = true to trigger expansion hydration
            10,
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

    let results = resp.results;
    println!("Search Results:");
    for r in &results {
        println!(" - Title: {}, ID: {}", r.title, r.id);
        if let Some(ref rels) = r.related_nodes {
            for rel in rels {
                println!("    -> Related Title: {}, ID: {}", rel.title, rel.id);
            }
        }
    }

    // Verify ep1 is the primary result
    assert!(!results.is_empty(), "Should return results");
    let primary = &results[0];
    assert_eq!(
        primary.id, ep1_id,
        "Primary result should be Step 1 Compile Action"
    );

    // Recursively collect all returned episode IDs (including related_nodes)
    let mut all_ids = std::collections::HashSet::new();
    for r in &results {
        all_ids.insert(r.id.clone());
        if let Some(ref rels) = r.related_nodes {
            for rel in rels {
                all_ids.insert(rel.id.clone());
            }
        }
    }

    // Verify all neighbors are expanded and returned
    assert!(
        all_ids.contains(&ep0_id),
        "Should return preceding neighbor Step 0"
    );
    assert!(
        all_ids.contains(&ep2_id),
        "Should return succeeding neighbor Step 2"
    );
    assert!(
        all_ids.contains(&ep3_id),
        "Should return succeeding neighbor Step 3 (depth 2)"
    );

    Ok(())
}
