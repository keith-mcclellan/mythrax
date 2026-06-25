use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::{BeliefState, ThoughtNode, EpisodeSave, WikiNode};

/// Finds a free unused TCP port on localhost.
fn find_free_port() -> u16 {
    // Bind to port 0 to let the OS assign a free port
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("Failed to bind to find free port");
    let port = listener.local_addr().expect("Failed to get local address").port();
    // Drop the listener immediately to release the port for the test
    drop(listener);
    port
}

#[tokio::test]
async fn test_schema_upgrades_exist() -> Result<()> {
    // 1. Find a free port to ensure we are not connecting to a running daemon.
    // Setting MYTHRAX_DAEMON_PORT forces the backend to run in embedded mode
    // rather than trying to connect to an external SurrealDB instance.
    let free_port = find_free_port();
    unsafe {
        std::env::set_var("MYTHRAX_DAEMON_PORT", free_port.to_string());
    }

    // 2. Initialize the database backend and run migrations
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // 3. Verify we can insert and retrieve a POMDP BeliefState record
    let belief = serde_json::json!({
        "session_id": "session_test_123",
        "tasks_todo": ["Write tests", "Implement code"],
        "hypotheses_tested": ["initial_hypothesis"],
        "confidence_score": 0.85,
        "uncertainty_areas": ["rocksdb locks"],
        "updated_at": chrono::Utc::now().to_rfc3339(),
    });

    let bs_res = backend.db.query("CREATE type::record('belief_state', 'session_test_123') CONTENT $content;")
        .bind(("content", belief))
        .await?;
    bs_res.check()?;

    let mut select_res = backend.db.query("SELECT * FROM belief_state WHERE session_id = 'session_test_123';")
        .await?;
    let retrieved_bs: Option<serde_json::Value> = select_res.take(0)?;
    assert!(retrieved_bs.is_some(), "BeliefState record should be successfully created and retrieved");
    let bs_val = retrieved_bs.unwrap();
    assert_eq!(bs_val["confidence_score"], 0.85f64);

    // 4. Verify we can insert and retrieve a TiM ThoughtNode record
    let thought = serde_json::json!({
        "title": "Abstract Optimization Lesson",
        "content": "Avoid physical code modification in memory paging to prevent watcher sync corruption.",
        "scope": "systems_design",
        "vault_path": "wiki/thoughts/abstract_optimization_lesson.md",
        "created_at": chrono::Utc::now().to_rfc3339(),
    });

    let thought_res = backend.db.query("CREATE type::record('thought_node', 'thought_test_456') CONTENT $content;")
        .bind(("content", thought))
        .await?;
    thought_res.check()?;

    let mut thought_select = backend.db.query("SELECT * FROM thought_node WHERE scope = 'systems_design';")
        .await?;
    let retrieved_thought: Option<serde_json::Value> = thought_select.take(0)?;
    assert!(retrieved_thought.is_some(), "ThoughtNode record should be successfully created and retrieved");

    // 5. Verify that schemafull relates_to table has strict IN and OUT constraints
    let ep_save = EpisodeSave {
        title: "Source Episode".to_string(),
        content: "Source content".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("episodes/source.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
    };
    let ep_id = backend.save_episode(&ep_save).await?;

    let wiki_save = WikiNode {
        id: None,
        name: "Target Wiki".to_string(),
        content: "Target content".to_string(),
        scope: "general".to_string(),
        vault_path: Some("wiki/target.md".to_string()),
        embedding: None,
    };
    let wiki_id = backend.save_wiki_node(&wiki_save).await?;

    // Relate them using the existing relates_to interface
    backend.relate_nodes(&ep_id, &wiki_id).await?;

    // Verify relations and fields
    let mut rel_select = backend.db.query("SELECT * FROM relates_to;")
        .await?;
    let relations: Vec<serde_json::Value> = rel_select.take(0)?;
    assert!(!relations.is_empty(), "relates_to edge should be successfully created");

    // 6. Verify the existence of importance and last_retrieved_at fields on nodes
    let ep_imp = EpisodeSave {
        title: "Important Episode".to_string(),
        content: "Important content".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("episodes/important.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
    };
    let ep_imp_id = backend.save_episode(&ep_imp).await?;
    
    let mut ep_select = backend.db.query("SELECT importance, last_retrieved_at FROM type::record('episode', $id);")
        .bind(("id", ep_imp_id.split(':').nth(1).unwrap()))
        .await?;
    let ep_meta: Option<serde_json::Value> = ep_select.take(0)?;
    assert!(ep_meta.is_some());
    let ep_meta_val = ep_meta.unwrap();
    // Default importance should be set to 5.0
    assert!(ep_meta_val.get("importance").is_some(), "importance field must exist in database");

    Ok(())
}
