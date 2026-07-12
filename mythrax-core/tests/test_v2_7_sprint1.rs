use mythrax_core::contracts::{ModelTier, TaskArchetype, TaskProfile};
use mythrax_core::llm::router::route_task;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use anyhow::Result;
use tempfile::tempdir;
use std::sync::Mutex;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn test_routing_types_exist() {
    let profile = TaskProfile::new(TaskArchetype::Summarization)
        .with_tokens(100)
        .with_latency_sensitive(true);

    assert_eq!(profile.archetype, TaskArchetype::Summarization);
    assert_eq!(profile.estimated_tokens, Some(100));
    assert!(profile.latency_sensitive);

    let tier = ModelTier::Micro;
    assert_eq!(tier, ModelTier::Micro);
}

#[tokio::test]
async fn test_routing_heuristics() {
    let db = SurrealBackend::new_in_memory().await.unwrap();
    db.init().await.unwrap();

    // Summarization, latency sensitive, few tokens -> Micro
    let profile = TaskProfile::new(TaskArchetype::Summarization)
        .with_tokens(100)
        .with_latency_sensitive(true);
    let tier = route_task(&db, &profile).await;
    let (total_swap, _) = mythrax_core::llm::router::get_swap_usage().unwrap_or((0.0, 0.0));
    if total_swap >= 4000.0 {
        assert_eq!(tier, ModelTier::Cloud);
    } else {
        assert_eq!(tier, ModelTier::Micro);
    }

    // Code, heavy tokens -> Cloud
    let profile_code = TaskProfile::new(TaskArchetype::Code)
        .with_tokens(10000)
        .with_latency_sensitive(false);
    let tier_code = route_task(&db, &profile_code).await;
    if total_swap >= 4000.0 {
        assert_eq!(tier_code, ModelTier::Cloud);
    } else {
        assert_eq!(tier_code, ModelTier::Cloud);
    }

    // Reasoning, medium tokens -> Large or Cloud
    let profile_reason = TaskProfile::new(TaskArchetype::Reasoning)
        .with_tokens(1500)
        .with_latency_sensitive(false);
    let tier_reason = route_task(&db, &profile_reason).await;
    if total_swap >= 4000.0 {
        assert_eq!(tier_reason, ModelTier::Cloud);
    } else {
        assert_eq!(tier_reason, ModelTier::Large);
    }
}


#[test]
fn test_embedding_cache_lru_eviction() {
    // Clear the cache first to ensure a clean state
    mythrax_core::embeddings::clear_embedding_cache();

    // Assert initially empty
    assert_eq!(mythrax_core::embeddings::get_embedding_cache_len(), 0);

    // Insert 10,000 items
    for i in 0..10000 {
        let text = format!("key_{}", i);
        let embedding = vec![i as f32; 10];
        mythrax_core::embeddings::cache_embedding(text, embedding);
    }
    assert_eq!(mythrax_core::embeddings::get_embedding_cache_len(), 10000);

    // Access key_0 to make it recently used
    let _ = mythrax_core::embeddings::get_cached_embedding("key_0");

    // Insert 1 more item (total insertions 10,001, but size should stay capped at 10,000)
    mythrax_core::embeddings::cache_embedding("key_10000".to_string(), vec![10000.0; 10]);

    // Since key_0 was accessed, the next oldest was key_1, so key_1 should be evicted and key_0 should still exist!
    assert_eq!(mythrax_core::embeddings::get_embedding_cache_len(), 10000);
    assert!(mythrax_core::embeddings::get_cached_embedding("key_1").is_none());
    assert!(mythrax_core::embeddings::get_cached_embedding("key_0").is_some());
    
    // Insert 10,005 items in total, verifying size stays capped at 10,000
    for i in 10001..10005 {
        let text = format!("key_{}", i);
        let embedding = vec![i as f32; 10];
        mythrax_core::embeddings::cache_embedding(text, embedding);
    }
    assert_eq!(mythrax_core::embeddings::get_embedding_cache_len(), 10000);
}

#[tokio::test]
async fn test_tokio_spawn_semaphore_cap() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    // The semaphore should start with 10 permits.
    assert_eq!(backend.reinforcement_semaphore.available_permits(), 10);
    
    // Acquire 10 permits
    let mut permits = Vec::new();
    for _ in 0..10 {
        permits.push(backend.reinforcement_semaphore.clone().acquire_owned().await?);
    }
    
    // Now there are 0 permits available.
    assert_eq!(backend.reinforcement_semaphore.available_permits(), 0);
    
    // If we try to acquire another, it blocks/fails.
    assert!(backend.reinforcement_semaphore.try_acquire().is_err());
    
    Ok(())
}

#[tokio::test(start_paused = true)]
async fn test_vram_eviction_timeout() -> Result<()> {
    use mythrax_core::llm::{DynamicModelBroker, ModelTier};

    let temp = tempdir()?;
    let broker = DynamicModelBroker::new(temp.path().to_path_buf()).await?;

    // Load Tier 1 model
    let tier1_engine = broker.acquire_llm(ModelTier::Tier1).await?;
    
    // Hold a strong reference to Tier 1 model to simulate it blocking/failing to deallocate
    let _strong_ref = tier1_engine.clone();

    // Call acquire_llm for Tier 2. This would block forever without the timeout.
    // With the timeout, it should complete.
    let start = tokio::time::Instant::now();
    let tier2_engine = broker.acquire_llm(ModelTier::Tier2).await?;
    let elapsed = start.elapsed();

    // The timeout is 30 seconds, so elapsed should be at least 30 seconds (virtual time)
    assert!(elapsed >= std::time::Duration::from_secs(30));
    assert!(tier2_engine.name().contains("Qwen"));

    Ok(())
}

#[test]
fn test_episode_raw_conversion() {
    use mythrax_core::db::EpisodeRaw;
    use mythrax_core::contracts::Episode;
    use surrealdb::types::{RecordId, RecordIdKey};
    use chrono::Utc;

    let raw = EpisodeRaw {
        id: RecordId {
            table: "episode".into(),
            key: RecordIdKey::from("foo_id"),
        },
        title: "Test Title".to_string(),
        content: "Test Content".to_string(),
        source: Some("test_source".to_string()),
        scope: Some("test_scope".to_string()),
        vault_path: Some("test_vault_path".to_string()),
        embedding: Some(vec![1.0, 2.0, 3.0]),
        processed_in_dream: Some(true),
        source_episode: Some(RecordId {
            table: "episode".into(),
            key: RecordIdKey::from("parent_id"),
        }),
        last_retrieved_at: Some("2026-07-11T20:00:00Z".to_string()),
        utility: Some(42.5),
        archived: Some(false),
        archived_at: Some(chrono::DateTime::parse_from_rfc3339("2026-07-11T20:00:00Z").unwrap().with_timezone(&Utc)),
        discovery_tokens: Some(10),
        facts: Some(vec!["fact1".to_string()]),
        concepts: Some(vec!["concept1".to_string()]),
        files_read: Some(vec!["read1.txt".to_string()]),
        files_modified: Some(vec!["mod1.txt".to_string()]),
        session_id: Some("session123".to_string()),
        word_count: Some(500),
        node_type: Some("episode".to_string()),
        confidence: Some(0.95),
        importance: Some(5.0),
    };

    let episode = Episode::from(raw);
    assert_eq!(episode.id, Some("episode:foo_id".to_string()));
    assert_eq!(episode.title, "Test Title");
    assert_eq!(episode.content, "Test Content");
    assert_eq!(episode.source, Some("test_source".to_string()));
    assert_eq!(episode.scope, Some("test_scope".to_string()));
    assert_eq!(episode.vault_path, Some("test_vault_path".to_string()));
    assert_eq!(episode.embedding, Some(vec![1.0, 2.0, 3.0]));
    assert_eq!(episode.processed_in_dream, Some(true));
    assert_eq!(episode.source_episode, Some("episode:parent_id".to_string()));
    assert_eq!(episode.last_retrieved_at, Some("2026-07-11T20:00:00Z".to_string()));
    assert_eq!(episode.utility, Some(42.5));
    assert_eq!(episode.archived, Some(false));
    assert_eq!(episode.archived_at, Some("2026-07-11T20:00:00+00:00".to_string()));
    assert_eq!(episode.discovery_tokens, Some(10));
    assert_eq!(episode.facts, Some(vec!["fact1".to_string()]));
    assert_eq!(episode.concepts, Some(vec!["concept1".to_string()]));
    assert_eq!(episode.files_read, Some(vec!["read1.txt".to_string()]));
    assert_eq!(episode.files_modified, Some(vec!["mod1.txt".to_string()]));
    assert_eq!(episode.session_id, Some("session123".to_string()));
    assert_eq!(episode.word_count, Some(500));
    assert_eq!(episode.node_type, Some("episode".to_string()));
    assert_eq!(episode.confidence, Some(0.95));
    assert_eq!(episode.importance, Some(5.0));
}

#[test]
fn test_episode_save_builder() {
    use mythrax_core::contracts::{EpisodeSave, Entity};

    let entity = Entity {
        name: "TestEntity".to_string(),
        entity_type: "concept".to_string(),
        summary: "Summary of TestEntity".to_string(),
        labels: vec!["test".to_string()],
        scope: Some("test_scope".to_string()),
        vault_path: Some("vault/test.md".to_string()),
        embedding: None,
    };

    let save = EpisodeSave::builder("Title".to_string(), "Content".to_string())
        .scope(Some("scope1".to_string()))
        .vault_path(Some("path1".to_string()))
        .source_episode(Some("episode1".to_string()))
        .session_id(Some("session1".to_string()))
        .task_id(Some("task1".to_string()))
        .discovery_tokens(Some(100))
        .facts(Some(vec!["fact1".to_string()]))
        .concepts(Some(vec!["concept1".to_string()]))
        .files_read(Some(vec!["read1".to_string()]))
        .files_modified(Some(vec!["mod1".to_string()]))
        .node_type(Some("node1".to_string()))
        .confidence(Some(0.85))
        .created_at(Some("2026-07-11T20:00:00Z".to_string()))
        .entities(vec![entity.clone()])
        .build();

    assert_eq!(save.title, "Title");
    assert_eq!(save.content, "Content");
    assert_eq!(save.scope, Some("scope1".to_string()));
    assert_eq!(save.vault_path, Some("path1".to_string()));
    assert_eq!(save.source_episode, Some("episode1".to_string()));
    assert_eq!(save.session_id, Some("session1".to_string()));
    assert_eq!(save.task_id, Some("task1".to_string()));
    assert_eq!(save.discovery_tokens, Some(100));
    assert_eq!(save.facts, Some(vec!["fact1".to_string()]));
    assert_eq!(save.concepts, Some(vec!["concept1".to_string()]));
    assert_eq!(save.files_read, Some(vec!["read1".to_string()]));
    assert_eq!(save.files_modified, Some(vec!["mod1".to_string()]));
    assert_eq!(save.node_type, Some("node1".to_string()));
    assert_eq!(save.confidence, Some(0.85));
    assert_eq!(save.created_at, Some("2026-07-11T20:00:00Z".to_string()));
    assert_eq!(save.entities.len(), 1);
    assert_eq!(save.entities[0].name, "TestEntity");
}

#[tokio::test]
async fn test_spreading_activation_batch_set_equivalence() -> Result<()> {
    let _guard = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    use mythrax_core::contracts::EpisodeSave;

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    backend.set_search_mode("keyword").await;
    backend.save_profile_key("search.enable_calibrated_confidence", "false").await?;
    backend.save_profile_key("search.enable_gaussian_temporal", "false").await?;
    backend.save_profile_key("search.enable_spreading_activation", "true").await?;
    backend.save_profile_key("search.spreading_activation_attenuation", "0.7").await?;

    // Insert an Entity
    let entity_uuid = uuid::Uuid::new_v4().to_string();
    let entity_id = format!("entity:{}", entity_uuid);
    backend.db.query("CREATE type::record('entity', $id) CONTENT { name: 'RustDB', entity_type: 'technology', summary: 'A database system written in Rust', labels: ['database'], scope: 'general' };")
        .bind(("id", entity_uuid.clone()))
        .await?.check()?;

    // Insert three Episodes
    let save1 = EpisodeSave::builder("Title1".to_string(), "Content1".to_string())
        .scope(Some("general".to_string()))
        .build();
    let ep1_id = backend.save_episode(&save1).await?;

    let save2 = EpisodeSave::builder("Title2".to_string(), "Content2".to_string())
        .scope(Some("general".to_string()))
        .build();
    let ep2_id = backend.save_episode(&save2).await?;

    // Relate Entity to Episodes
    backend.relate_nodes(&entity_id, &ep1_id, None, None, Some(0.8)).await?;
    backend.relate_nodes(&entity_id, &ep2_id, None, None, Some(0.6)).await?;

    // Run the batch query version by searching
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
        None,
        true,
        None,
    )).await?;

    // Find our episodes in the search results
    let r1 = resp.results.iter().find(|r| r.id == ep1_id).expect("ep1 should be found");
    let r2 = resp.results.iter().find(|r| r.id == ep2_id).expect("ep2 should be found");

    // Manually compute/simulate:
    // Similarity = 1.0 * confidence * attenuation
    // ep1: 1.0 * 0.8 * 0.7 = 0.56
    // ep2: 1.0 * 0.6 * 0.7 = 0.42
    assert!((r1.similarity - 0.56).abs() < 1e-4);
    assert!((r2.similarity - 0.42).abs() < 1e-4);

    Ok(())
}

#[test]
fn test_search_params_struct() {
    use mythrax_core::contracts::SearchParams;

    let default_params = SearchParams::default();
    assert_eq!(default_params.query, "");
    assert_eq!(default_params.scope, None);
    assert_eq!(default_params.deep_insight, false);
    assert_eq!(default_params.limit, 15);
    assert_eq!(default_params.offset, 0);
    assert_eq!(default_params.threshold, 0.55);
    assert_eq!(default_params.token_budget, None);
    assert_eq!(default_params.allow_downward, false);
    assert_eq!(default_params.include_episodes, false);
    assert_eq!(default_params.include_artifacts, false);
    assert_eq!(default_params.session_id, None);
    assert_eq!(default_params.include_archived, false);
    assert_eq!(default_params.temporal_anchor, None);

    let params = SearchParams::new("test_query")
        .scope("my_scope")
        .deep_insight(true)
        .limit(20)
        .offset(5)
        .threshold(0.8)
        .token_budget(1000)
        .allow_downward(true)
        .include_episodes(true)
        .include_artifacts(true)
        .session_id("session_123")
        .include_archived(true)
        .temporal_anchor("anchor_time");

    assert_eq!(params.query, "test_query");
    assert_eq!(params.scope, Some("my_scope".to_string()));
    assert_eq!(params.deep_insight, true);
    assert_eq!(params.limit, 20);
    assert_eq!(params.offset, 5);
    assert_eq!(params.threshold, 0.8);
    assert_eq!(params.token_budget, Some(1000));
    assert_eq!(params.allow_downward, true);
    assert_eq!(params.include_episodes, true);
    assert_eq!(params.include_artifacts, true);
    assert_eq!(params.session_id, Some("session_123".to_string()));
    assert_eq!(params.include_archived, true);
    assert_eq!(params.temporal_anchor, Some("anchor_time".to_string()));

    let positional = SearchParams::from_positional(
        "pos_query",
        Some("pos_scope"),
        true,
        25,
        10,
        0.7,
        Some(500),
        true,
        true,
        true,
        Some("pos_session"),
        true,
        Some("pos_anchor"),
    );

    assert_eq!(positional.query, "pos_query");
    assert_eq!(positional.scope, Some("pos_scope".to_string()));
    assert_eq!(positional.deep_insight, true);
    assert_eq!(positional.limit, 25);
    assert_eq!(positional.offset, 10);
    assert_eq!(positional.threshold, 0.7);
    assert_eq!(positional.token_budget, Some(500));
    assert_eq!(positional.allow_downward, true);
    assert_eq!(positional.include_episodes, true);
    assert_eq!(positional.include_artifacts, true);
    assert_eq!(positional.session_id, Some("pos_session".to_string()));
    assert_eq!(positional.include_archived, true);
    assert_eq!(positional.temporal_anchor, Some("pos_anchor".to_string()));
}

#[test]
fn test_strip_code_fences_all_variants() {
    use mythrax_core::llm::strip_code_fences;

    // Normal fences with language suffix
    assert_eq!(strip_code_fences("```json\n{\"a\": 1}\n```"), "{\"a\": 1}");
    
    // Normal fences without language suffix
    assert_eq!(strip_code_fences("```\n{\"a\": 1}\n```"), "{\"a\": 1}");
    
    // Multi-line fences
    assert_eq!(strip_code_fences("```json\n{\n  \"a\": 1\n}\n```"), "{\n  \"a\": 1\n}");

    // Fences without newlines
    assert_eq!(strip_code_fences("```json{\"a\": 1}```"), "{\"a\": 1}");
    assert_eq!(strip_code_fences("```{\"a\": 1}```"), "{\"a\": 1}");

    // Fences with leading/trailing whitespace
    assert_eq!(strip_code_fences("  \n  ```json\n{\"a\": 1}\n```  \n "), "{\"a\": 1}");

    // Bare strings (returns unchanged)
    assert_eq!(strip_code_fences("{\"a\": 1}"), "{\"a\": 1}");
    assert_eq!(strip_code_fences("plain text"), "plain text");

    // Nested fences (strips only outer fences)
    let nested = "```markdown\nOuter\n```rust\nfn inner() {}\n```\nMore Outer\n```";
    let expected = "Outer\n```rust\nfn inner() {}\n```\nMore Outer";
    assert_eq!(strip_code_fences(nested), expected);
}

#[test]
fn test_normalized_embedding_invariant() {
    use mythrax_core::embeddings::NormalizedEmbedding;

    // Vector is empty
    let empty_vec: Vec<f32> = vec![];
    assert!(NormalizedEmbedding::try_new(empty_vec).is_err());

    // Valid normalized vector (magnitude exactly 1.0)
    let valid_vec = vec![0.6, 0.8]; // 0.6^2 + 0.8^2 = 0.36 + 0.64 = 1.0
    let norm1 = NormalizedEmbedding::try_new(valid_vec.clone());
    assert!(norm1.is_ok());
    let norm1 = norm1.unwrap();
    assert_eq!(norm1.as_slice(), &valid_vec);
    assert_eq!(norm1.clone().into_inner(), valid_vec);

    // Magnitude within 1% of 1.0
    let valid_vec_high = vec![0.6 * 1.009, 0.8 * 1.009];
    assert!(NormalizedEmbedding::try_new(valid_vec_high).is_ok());
    
    let valid_vec_low = vec![0.6 * 0.991, 0.8 * 0.991];
    assert!(NormalizedEmbedding::try_new(valid_vec_low).is_ok());

    // Non-normalized vector (too small, magnitude = 0.5)
    let small_vec = vec![0.3, 0.4];
    assert!(NormalizedEmbedding::try_new(small_vec).is_err());

    // Non-normalized vector (too large, magnitude = 2.0)
    let large_vec = vec![1.2, 1.6];
    assert!(NormalizedEmbedding::try_new(large_vec).is_err());

    // Dot product calculation
    let norm2 = NormalizedEmbedding::try_new(vec![0.6, 0.8]).unwrap();
    let dot = norm1.dot_product(&norm2);
    assert!((dot - 1.0).abs() < 1e-5);

    // dot_product([0.6, 0.8], [-0.8, 0.6]) = -0.48 + 0.48 = 0.0 (orthogonal)
    let norm_ortho = NormalizedEmbedding::try_new(vec![-0.8, 0.6]).unwrap();
    let dot_ortho = norm1.dot_product(&norm_ortho);
    assert!(dot_ortho.abs() < 1e-5);
}

#[test]
fn test_tier_enum_roundtrip() {
    use mythrax_core::contracts::Tier;

    // Check FromStr mapping (all variants and aliases)
    assert_eq!("permanent".parse::<Tier>().unwrap(), Tier::Wisdom);
    assert_eq!("skills".parse::<Tier>().unwrap(), Tier::Wisdom);
    assert_eq!("wisdom".parse::<Tier>().unwrap(), Tier::Wisdom);

    assert_eq!("dynamic".parse::<Tier>().unwrap(), Tier::Project);
    assert_eq!("forge".parse::<Tier>().unwrap(), Tier::Project);
    assert_eq!("project".parse::<Tier>().unwrap(), Tier::Project);

    assert_eq!("user".parse::<Tier>().unwrap(), Tier::User);

    assert_eq!("session".parse::<Tier>().unwrap(), Tier::Session);

    assert_eq!("working".parse::<Tier>().unwrap(), Tier::Working);
    assert_eq!("stm".parse::<Tier>().unwrap(), Tier::Working);

    // Invalid mapping
    assert!("invalid_tier".parse::<Tier>().is_err());

    // Check Display mapping
    assert_eq!(Tier::Wisdom.to_string(), "wisdom");
    assert_eq!(Tier::Project.to_string(), "project");
    assert_eq!(Tier::User.to_string(), "user");
    assert_eq!(Tier::Session.to_string(), "session");
    assert_eq!(Tier::Working.to_string(), "working");

    // Check Serde serialization/deserialization roundtrip
    for variant in [Tier::Wisdom, Tier::Project, Tier::User, Tier::Session, Tier::Working] {
        let serialized = serde_json::to_string(&variant).unwrap();
        assert_eq!(serialized, format!("\"{}\"", variant));
        let deserialized: Tier = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, variant);
    }

    // Verify raw db strings deserializing correctly to the corresponding Tier enum
    assert_eq!(serde_json::from_str::<Tier>("\"permanent\"").unwrap(), Tier::Wisdom);
    assert_eq!(serde_json::from_str::<Tier>("\"dynamic\"").unwrap(), Tier::Project);
    assert_eq!(serde_json::from_str::<Tier>("\"stm\"").unwrap(), Tier::Working);
}
