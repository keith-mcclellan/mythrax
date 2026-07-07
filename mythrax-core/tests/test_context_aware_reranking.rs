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

