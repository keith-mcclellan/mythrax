use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::EpisodeSave;

#[tokio::test]
async fn test_user_profile_compilation_and_sorting() {
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    let session_id = "test_session_123";

    // 1. Save STM facts
    backend.save_stm(session_id, "favorite_color", "blue").await.unwrap();
    backend.save_stm(session_id, "degree", "physics").await.unwrap();

    // 2. Save episodes with out-of-order and identical timestamps (as done in transaction batching)
    // We name the title with numeric Turn indices.
    let ep1 = EpisodeSave {
        title: format!("{} - Turn 1", session_id),
        content: "I started my study.".to_string(),
        session_id: Some(session_id.to_string()),
        node_type: Some("user_input".to_string()),
        ..Default::default()
    };
    let ep2 = EpisodeSave {
        title: format!("{} - Turn 2", session_id),
        content: "I prefer coffee over tea.".to_string(),
        session_id: Some(session_id.to_string()),
        node_type: Some("user_input".to_string()),
        ..Default::default()
    };
    let ep10 = EpisodeSave {
        title: format!("{} - Turn 10", session_id),
        content: "I live in Boston.".to_string(),
        session_id: Some(session_id.to_string()),
        node_type: Some("user_input".to_string()),
        ..Default::default()
    };
    let ep3 = EpisodeSave {
        title: format!("{} - Turn 3", session_id),
        content: "My occupation is a software engineer.".to_string(),
        session_id: Some(session_id.to_string()),
        node_type: Some("user_input".to_string()),
        ..Default::default()
    };

    backend.save_episode(&ep2).await.unwrap();
    backend.save_episode(&ep10).await.unwrap();
    backend.save_episode(&ep1).await.unwrap();
    backend.save_episode(&ep3).await.unwrap();

    // Compile profile with limit=0 (no truncation)
    backend.save_profile_key("search.user_profile_max_len", "0").await.unwrap();
    let profile = backend.compile_user_profile(session_id).await.unwrap();

    // The output should sort the user turns chronologically (1 -> 2 -> 3 -> 10)
    // and append the STM facts (sorted key alphabetically) at the end.
    let expected = vec![
        "I started my study.",
        "I prefer coffee over tea.",
        "My occupation is a software engineer.",
        "I live in Boston.",
        "degree: physics",
        "favorite_color: blue",
    ].join("\n");

    assert_eq!(profile.trim(), expected.trim());
}

#[tokio::test]
async fn test_user_profile_smart_truncation() {
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    let session_id = "test_session_456";

    // STM facts: 40 chars
    backend.save_stm(session_id, "deg", "math").await.unwrap(); // deg: math (9 chars)
    backend.save_stm(session_id, "fav", "red").await.unwrap();  // fav: red (8 chars)

    // User turns:
    // Turn 1: 15 chars
    let ep1 = EpisodeSave {
        title: format!("{} - Turn 1", session_id),
        content: "Hello my friend".to_string(),
        session_id: Some(session_id.to_string()),
        node_type: Some("user_input".to_string()),
        ..Default::default()
    };
    // Turn 2: 24 chars
    let ep2 = EpisodeSave {
        title: format!("{} - Turn 2", session_id),
        content: "Weather is nice today so".to_string(),
        session_id: Some(session_id.to_string()),
        node_type: Some("user_input".to_string()),
        ..Default::default()
    };
    // Turn 3: 20 chars
    let ep3 = EpisodeSave {
        title: format!("{} - Turn 3", session_id),
        content: "I went for a walk to".to_string(),
        session_id: Some(session_id.to_string()),
        node_type: Some("user_input".to_string()),
        ..Default::default()
    };

    backend.save_episode(&ep1).await.unwrap();
    backend.save_episode(&ep2).await.unwrap();
    backend.save_episode(&ep3).await.unwrap();

    // Truncate to max 65 characters.
    // STM: "deg: math\nfav: red" (17 chars).
    // Remaining length for turns: 65 - 18 = 47 chars.
    // Turns from newest to oldest: Turn 3 (20 chars), Turn 2 (24 chars), Turn 1 (15 chars).
    // Can we fit Turn 3? Yes (20 chars, total 17 + 1 + 20 = 38).
    // Can we fit Turn 2? Yes (24 chars, total 38 + 1 + 24 = 63).
    // Can we fit Turn 1? No (15 chars, 63 + 1 + 15 = 79 > 65).
    // So turns kept: Turn 2, Turn 3.
    // Re-reversed chronologically: Turn 2 -> Turn 3.
    // Expected output: "Weather is nice today so\nI went for a walk to\ndeg: math\nfav: red";
    backend.save_profile_key("search.user_profile_max_len", "65").await.unwrap();
    let profile = backend.compile_user_profile(session_id).await.unwrap();

    let expected = "Weather is nice today so\nI went for a walk to\ndeg: math\nfav: red";
    assert_eq!(profile.trim(), expected);
}

#[tokio::test]
async fn test_pipeline_retrieval_optimizations() {
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    // Verify default TF-IDF pool size configuration can be queried
    backend.save_profile_key("search.tfidf_pool_size", "100").await.unwrap();
    let tfidf_pool = backend.get_profile_key("search.tfidf_pool_size").await.unwrap();
    assert_eq!(tfidf_pool.unwrap(), "100");
}

#[tokio::test]
async fn test_dynamic_ladder_boost_scaling() {
    // Force mock behavior to bypass embedding generation and set predictable raw similarities
    unsafe {
        std::env::set_var("MYTHRAX_SIGMOID_GATED_SEARCH_TEST", "true");
    }
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();
    backend.save_profile_key("search.enable_access_reinforcement", "false").await.unwrap();
    
    // Save a mock episode with query-matching content and title forced to 0.85 similarity
    let ep = EpisodeSave {
        title: "High Similarity Old Node".to_string(),
        content: "Rust database locks and transaction management.".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let ep_id = backend.save_episode(&ep).await.unwrap();
    
    // 1. Scale = 0.0 (no boost) -> raw_vector_sim should be exactly 0.85
    backend.save_profile_key("search.ladder_scale", "0.0").await.unwrap();
    let res = backend.search(
        "Rust database locks",
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
    ).await.unwrap();
    assert!(!res.results.is_empty());
    let r = res.results.iter().find(|x| x.id == ep_id).unwrap();
    assert_eq!(r.raw_vector_sim.unwrap(), 0.85f32);
    
    // 2. Scale = 1.0 (full boost) -> raw_vector_sim should be 0.85 + 0.15 = 1.0
    backend.save_profile_key("search.ladder_scale", "1.0").await.unwrap();
    let res2 = backend.search(
        "Rust database locks",
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
    ).await.unwrap();
    let r2 = res2.results.iter().find(|x| x.id == ep_id).unwrap();
    assert_eq!(r2.raw_vector_sim.unwrap(), 1.0f32);
    
    // 3. Scale = 0.5 (half boost) -> raw_vector_sim should be 0.85 + 0.075 = 0.925
    backend.save_profile_key("search.ladder_scale", "0.5").await.unwrap();
    let res3 = backend.search(
        "Rust database locks",
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
    ).await.unwrap();
    let r3 = res3.results.iter().find(|x| x.id == ep_id).unwrap();
    assert!((r3.raw_vector_sim.unwrap() - 0.925f32).abs() < 1e-5);
}

#[tokio::test]
async fn test_dynamic_temporal_decay_floor() {
    unsafe {
        std::env::set_var("MYTHRAX_SIGMOID_GATED_SEARCH_TEST", "true");
    }
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();
    backend.save_profile_key("search.enable_access_reinforcement", "false").await.unwrap();
    
    // Save a mock episode with query-matching content
    let ep = EpisodeSave {
        title: "High Similarity Old Node".to_string(),
        content: "Rust database locks and transaction management.".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let ep_id = backend.save_episode(&ep).await.unwrap();
    let uuid = ep_id.split(':').nth(1).unwrap();
    
    // Update created_at and clear last_retrieved_at to force decay fallback to created_at
    backend.db.query("UPDATE type::record('episode', $id) MERGE { created_at: time::now() - 365d, last_retrieved_at: NONE };")
        .bind(("id", uuid))
        .await.unwrap().check().unwrap();
        
    // 1. Decay floor = 0.20 -> factor_multiplier should be 0.25 + 0.5 * 0.20 = 0.35
    backend.save_profile_key("search.temporal_decay_floor", "0.20").await.unwrap();
    let res = backend.search(
        "Rust database locks",
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
    ).await.unwrap();
    let r = res.results.iter().find(|x| x.id == ep_id).unwrap();
    assert!((r.factor_multiplier.unwrap() - 0.35f32).abs() < 1e-4);
    
    // Reset created_at and clear last_retrieved_at again to force decay on the second search
    backend.db.query("UPDATE type::record('episode', $id) MERGE { created_at: time::now() - 365d, last_retrieved_at: NONE };")
        .bind(("id", uuid))
        .await.unwrap().check().unwrap();

    // 2. Decay floor = 0.45 -> factor_multiplier should be 0.25 + 0.5 * 0.45 = 0.475
    backend.save_profile_key("search.temporal_decay_floor", "0.45").await.unwrap();
    let res2 = backend.search(
        "Rust database locks",
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
    ).await.unwrap();
    let r2 = res2.results.iter().find(|x| x.id == ep_id).unwrap();
    assert!((r2.factor_multiplier.unwrap() - 0.475f32).abs() < 1e-4);
}

