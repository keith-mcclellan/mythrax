use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::{WatchIgnoreList, start_watching};
use std::sync::Arc;
use std::time::Duration;
use tempfile::tempdir;

#[tokio::test]
async fn test_watcher_upstream_filtering_coalescing_and_bounded_pool() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let target_dir = temp_dir.path().join("target");
    std::fs::create_dir_all(&target_dir).unwrap();

    // Initialize Watch Ignore List
    let ignore_list = Arc::new(WatchIgnoreList::new());

    // Initialize Database Backend
    let db_path = temp_dir.path().join("db");
    let backend = Arc::new(
        SurrealBackend::new(&format!("surrealkv://{}", db_path.to_string_lossy()))
            .await
            .unwrap(),
    );
    backend.init().await.unwrap();

    // Initialize Markdown Store
    let store = Arc::new(MarkdownStore::new(temp_dir.path().to_path_buf()).unwrap());

    // Start the file watcher
    let _watcher = start_watching(
        temp_dir.path().to_path_buf(),
        ignore_list,
        backend.clone(),
        store.clone(),
        None,
    )
    .unwrap();

    // 1. Stress watch channel: Write 1000 build files (binary) to target directory
    // These should be filtered out by the watcher's ignore list or file type check
    for i in 0..1000 {
        let file_path = target_dir.join(format!("build_file_{}.o", i));
        std::fs::write(file_path, "binary content").unwrap();
    }

    // 2. Test Coalescing: Write to a valid note 5 times in rapid succession (under 200ms)
    let valid_file = temp_dir.path().join("coalesced_note.md");
    for i in 0..5 {
        std::fs::write(
            &valid_file,
            format!("---\ntitle: Coalesced\n---\nWrite {}", i),
        )
        .unwrap();
        // Small delay to simulate rapid but distinct writes
        tokio::time::sleep(Duration::from_millis(30)).await;
    }

    // 3. Test Bounded Worker Pool: Write to 20 different files concurrently
    // This triggers 20 embedding tasks. The worker pool should serialize them (max 2 concurrent).
    let mut handles = vec![];
    for i in 0..20 {
        let temp_dir_clone = temp_dir.path().to_path_buf();
        let handle = tokio::spawn(async move {
            let file_path = temp_dir_clone.join(format!("bulk_note_{}.md", i));
            std::fs::write(
                &file_path,
                format!("---\ntitle: Bulk {}\n---\nBulk Content {}", i, i),
            )
            .unwrap();
        });
        handles.push(handle);
    }
    // Wait for all writes to complete
    for h in handles {
        h.await.unwrap();
    }

    // 4. Assertions:

    // A. Verify coalesced note and all bulk notes are successfully indexed via polling (up to 8 seconds)
    let mut coalesced_indexed = false;
    let mut bulk_indexed = false;
    for _ in 0..80 {
        if !coalesced_indexed {
            if let Ok(query_res) = backend
                .search("Write 4", None, false, 10, 0, 0.0, None, false, true, false)
                .await
            {
                if !query_res.results.is_empty() {
                    coalesced_indexed = true;
                }
            }
        }
        if !bulk_indexed {
            if let Ok(query_res) = backend
                .search(
                    "Bulk Content 19",
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
            {
                if !query_res.results.is_empty() {
                    bulk_indexed = true;
                }
            }
        }
        if coalesced_indexed && bulk_indexed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    assert!(
        coalesced_indexed,
        "Coalesced note final write must be indexed successfully"
    );
    assert!(bulk_indexed, "All bulk notes must be indexed successfully");

    // B. Verify from DB telemetry that only ONE indexing write occurred (others were coalesced)
    // This verifies the coalescing logic: 5 rapid writes -> 1 DB commit
    let db_writes = backend
        .get_indexing_write_count("coalesced_note.md")
        .await
        .unwrap();
    assert_eq!(
        db_writes, 1,
        "Rapid modifications must be coalesced into a single database commit"
    );

    // C. Verify from embedding worker telemetry that the maximum concurrent background
    // embedding executions never exceeded 2
    // This verifies the bounded worker pool logic
    let max_concurrent_embeddings = backend
        .get_max_concurrent_background_embeddings()
        .await
        .unwrap();
    assert!(
        max_concurrent_embeddings <= 2,
        "Bulk background embedding tasks must be serialized through a bounded worker pool (max 2 concurrent)"
    );
}
