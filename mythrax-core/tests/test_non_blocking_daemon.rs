use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn test_thread_safe_wal_concurrency_and_robust_replay_marker_compaction() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("db");
    let wal_path = temp_dir.path().join("episodes.jsonl");
    let initialized_marker = db_path.join(".initialized");

    // 1. Boot primary backend
    let backend = SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy()))
        .await
        .unwrap();
    backend.init().await.unwrap();

    assert!(initialized_marker.exists());

    // 2. Simulate concurrent saves: write 10 episodes in parallel
    let mut handles = vec![];
    for i in 0..10 {
        let backend_clone = backend.clone();
        let wal_clone = wal_path.clone();
        let handle = tokio::spawn(async move {
            let episode = EpisodeSave {
                title: format!("Concurrent Episode {}", i),
                content: format!("Content {}", i),
                entities: vec![],
                scope: Some("general".to_string()),
                vault_path: Some(format!("notes/note_{}.md", i)),
                source_episode: None,
                session_id: Some("test_session".to_string()),
                task_id: None,
                ..Default::default()
            };
            backend_clone
                .save_episode_with_wal_actor(&episode, &wal_clone)
                .await
                .unwrap();
        });
        handles.push(handle);
    }
    for h in handles {
        h.await.unwrap();
    }

    // Give background WAL actor time to write and flush
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify WAL has 10 valid lines
    let wal_content = std::fs::read_to_string(&wal_path).unwrap();
    let lines_count = wal_content.lines().count();
    assert_eq!(lines_count, 10);

    // 3. Save a duplicate update to verify WAL compaction
    let duplicate_episode = EpisodeSave {
        title: "Concurrent Episode 0".to_string(), // Duplicate ID/Title
        content: "Updated Content 0".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/note_0.md".to_string()),
        source_episode: None,
        session_id: Some("test_session".to_string()),
        task_id: None,
        ..Default::default()
    };
    backend
        .save_episode_with_wal_actor(&duplicate_episode, &wal_path)
        .await
        .unwrap();

    // Give background WAL actor time to write and flush
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Verify raw WAL lines count is now 11
    let wal_content_2 = std::fs::read_to_string(&wal_path).unwrap();
    assert_eq!(wal_content_2.lines().count(), 11);

    // Trigger WAL Compaction (simulating background dreaming compaction)
    backend.compact_wal_file(&wal_path).await.unwrap();

    // Assert that the compacted WAL file contains exactly 10 lines (the duplicate was collapsed to its latest version)
    let compacted_content = std::fs::read_to_string(&wal_path).unwrap();
    assert_eq!(
        compacted_content.lines().count(),
        10,
        "WAL compaction must collapse duplicate records to their latest state"
    );

    // 4. Inject a corrupted / malformed line to verify robust WAL recovery parsing
    let mut wal_file = std::fs::OpenOptions::new()
        .append(true)
        .open(&wal_path)
        .unwrap();
    use std::io::Write;
    writeln!(wal_file, "{{ \"title\": \"Malformed JSON\", \"content\": ").unwrap();

    // 5. Test Recovery Replay Trigger:
    // Drop backend, delete the database directory BUT keep the WAL log file
    drop(backend);
    std::fs::remove_dir_all(&db_path).unwrap();

    assert!(!initialized_marker.exists());

    // Boot fresh backend
    let recovered_backend =
        SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy()))
            .await
            .unwrap();
    recovered_backend.init().await.unwrap();

    // Trigger recovery replay: must successfully replay the 10 episodes, skip the malformed line with a warning, and write the marker
    recovered_backend
        .replay_wal_if_fresh(&wal_path, &initialized_marker)
        .await
        .unwrap();
    assert!(initialized_marker.exists());

    // Verify all 10 episodes were successfully recovered
    for i in 0..10 {
        let search_res = recovered_backend
            .search(
                &format!("Concurrent Episode {}", i),
                None,
                false,
                10,
                0,
                0.0,
                None,
                false,
                true,
                false,
            )
            .await
            .unwrap();
        assert!(
            !search_res.results.is_empty(),
            "Episode {} must be recovered successfully",
            i
        );
    }
}
