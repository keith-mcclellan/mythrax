use anyhow::Result;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::contracts::EpisodeSave;
use surrealdb_types::SurrealValue;

#[tokio::test]
async fn test_v2_5_2_retrieval_signals_integration() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    backend.save_profile_key("search.enable_calibrated_confidence", "false").await?;
    backend.save_profile_key("search.enable_gaussian_temporal", "false").await?;

    // --- TASK A.6: Concept Spreading Activation ---
    // 1. Enable Spreading Activation
    backend.save_profile_key("search.enable_spreading_activation", "true").await?;

    // 2. Insert an Entity as an anchor point
    let entity_uuid = uuid::Uuid::new_v4().to_string();
    let entity_id = format!("entity:{}", entity_uuid);
    backend.db.query("CREATE type::record('entity', $id) CONTENT { name: 'RustDB', entity_type: 'technology', summary: 'A database system written in Rust', labels: ['database'], scope: 'general' };")
        .bind(("id", entity_uuid.clone()))
        .await?.check()?;

    // 3. Insert an Episode that relates to the Entity
    let ep = EpisodeSave {
        created_at: None,
        title: "Database Transaction Isolation".to_string(),
        content: "We need to ensure strict session isolation in our database adapter.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("episodes/tx_isolation.md".to_string()),
        session_id: Some("session_foo".to_string()),
        ..Default::default()
    };
    let ep_id_str = backend.save_episode(&ep).await?;

    // 4. Relate Entity -> relates_to -> Episode with a confidence of 0.8
    backend.relate_nodes(&entity_id, &ep_id_str, None, None, Some(0.8)).await?;

    // 5. Search for "RustDB"
    let resp = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "RustDB",
        Some("general"),
        false,
        10,
        0,
        0.0,
        None,
        false,
        true,
        true,
        Some("session_foo"),
        true,
        None,
    )).await?;

    // The Episode is not retrieved by the direct keyword/vector search for "RustDB", but is traversed via relates_to edge!
    let found_activation = resp.results.iter().any(|r| r.id == ep_id_str);
    assert!(found_activation, "Episode should be retrieved via Spreading Activation");

    // Let's verify that disabling the feature prevents it from being retrieved
    backend.save_profile_key("search.enable_spreading_activation", "false").await?;
    let resp_disabled = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "RustDB",
        Some("general"),
        false,
        10,
        0,
        0.0,
        None,
        false,
        true,
        true,
        Some("session_foo"),
        true,
        None,
    )).await?;
    let found_disabled = resp_disabled.results.iter().any(|r| r.id == ep_id_str);
    assert!(!found_disabled, "Episode should NOT be retrieved when Spreading Activation is disabled");


    // --- TASK A.7: STM Working Memory Injection ---
    // 1. Enable STM Retrieval
    backend.save_profile_key("search.enable_stm_retrieval", "true").await?;

    // 2. Put key-value pair in short-term memory
    backend.save_stm("session_bar", "context_guard", "Avoid concurrent RocksDB process lock by starting in client mode").await?;

    // 3. Search under "session_bar" with a query related to the STM content
    let resp_stm = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "RocksDB process lock client mode",
        Some("general"),
        false,
        10,
        0,
        0.0,
        None,
        false,
        true,
        true,
        Some("session_bar"),
        true,
        None,
    )).await?;

    // Verify that the STM record is injected with working tier, synthetic ID, and utility = 100.0
    let match_stm = resp_stm.results.iter().find(|r| r.id == "stm:session_bar:context_guard");
    assert!(match_stm.is_some(), "STM entry must be injected into search results");
    let stm_res = match_stm.unwrap();
    assert_eq!(stm_res.tier, mythrax_core::contracts::Tier::Working);
    assert_eq!(stm_res.utility, 100.0);
    assert_eq!(stm_res.title, "context_guard");
    assert_eq!(stm_res.content, "Avoid concurrent RocksDB process lock by starting in client mode");


    // --- TASK A.8: Access-Driven Utility Reinforcement ---
    // 1. Enable Access Reinforcement
    backend.save_profile_key("search.enable_access_reinforcement", "true").await?;

    // 2. Insert a new Episode for reinforcement testing
    let ep_reinforce = EpisodeSave {
        created_at: None,
        title: "Memory Leak Diagnostics".to_string(),
        content: "Identify JavaScript memory leaks using Chrome DevTools heap snapshots.".to_string(),
        scope: Some("general".to_string()),
        vault_path: Some("episodes/mem_leak.md".to_string()),
        ..Default::default()
    };
    let ep_re_id = backend.save_episode(&ep_reinforce).await?;
    let ep_re_uuid = ep_re_id.split(':').nth(1).unwrap();

    // 3. Perform a search to retrieve the episode
    let _resp_re = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "Memory Leak Diagnostics",
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

    // Give the async spawned background task a moment to execute
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Check if the metrics record was inserted
    #[derive(serde::Deserialize, surrealdb_types::SurrealValue, Debug)]
    struct LocalMetricsRow {
        utility_score: f64,
        access_count: i64,
    }

    let check_sql = "SELECT utility_score, access_count FROM metrics WHERE target_id = type::record('episode', $ep_id) LIMIT 1;";
    let mut metrics_res = backend.db.query(check_sql).bind(("ep_id", ep_re_uuid)).await?.check()?;
    let mut rows: Vec<LocalMetricsRow> = metrics_res.take(0)?;
    assert_eq!(rows.len(), 1, "Metrics record should be created on first access");
    let initial_row = rows.pop().unwrap();
    assert_eq!(initial_row.access_count, 1);
    assert_eq!(initial_row.utility_score, 50.0);

    // 4. Perform search again to increment access count and trigger reinforcement logic
    let _resp_re2 = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "Memory Leak Diagnostics",
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

    // Give background task a moment
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let mut metrics_res2 = backend.db.query(check_sql).bind(("ep_id", ep_re_uuid)).await?.check()?;
    let mut rows2: Vec<LocalMetricsRow> = metrics_res2.take(0)?;
    assert_eq!(rows2.len(), 1);
    let updated_row = rows2.pop().unwrap();
    assert_eq!(updated_row.access_count, 2, "Access count should be incremented to 2");
    
    // utility_reinforced = (50.0 + log2(2) * exp(0)) = 50.0 + 1.0 = 51.0
    assert!(
        (updated_row.utility_score - 51.0).abs() < 0.1,
        "Utility score should be reinforced to approximately 51.0, got: {}",
        updated_row.utility_score
    );

    // --- TASK B.3: MLX Cross-Encoder Reranker (Mocked in test mode) ---
    // 1. Enable Cross-Encoder Reranking
    backend.save_profile_key("search.enable_cross_encoder_rerank", "true").await?;
    backend.save_profile_key("search.mock_reranker", "true").await?;
    backend.save_profile_key("search.rerank_pool_size", "5").await?;

    // 2. Perform a search with two episodes in candidates
    let ep_other = EpisodeSave {
        created_at: None,
        title: "Random unrelated title".to_string(),
        content: "Totally unrelated document content that does not match transaction isolation.".to_string(),
        scope: Some("general".to_string()),
        vault_path: Some("episodes/random.md".to_string()),
        session_id: Some("session_foo".to_string()),
        ..Default::default()
    };
    let _ep_other_id = backend.save_episode(&ep_other).await?;

    let resp_rerank = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "Database Transaction Isolation",
        Some("general"),
        false,
        5,
        0,
        0.0,
        None,
        false,
        true,
        true,
        Some("session_foo"),
        true,
        None,
    )).await?;

    // Verify that the first candidate (which matches the mock boost) has similarity 0.95
    assert!(!resp_rerank.results.is_empty());
    assert_eq!(resp_rerank.results[0].similarity, 0.95f32);
    
    // Disabling the reranker should run without setting similarity to 0.95
    backend.save_profile_key("search.enable_cross_encoder_rerank", "false").await?;
    backend.save_profile_key("search.mock_reranker", "false").await?;
    let resp_no_rerank = backend.search(mythrax_core::contracts::SearchParams::from_positional(
        "Database Transaction Isolation",
        Some("general"),
        false,
        5,
        0,
        0.0,
        None,
        false,
        true,
        true,
        Some("session_foo"),
        true,
        None,
    )).await?;
    assert_ne!(resp_no_rerank.results[0].similarity, 0.95f32);

    Ok(())
}
