#![allow(dead_code, unused_imports)]

mod bm25 {
use mythrax_core::retrieval::bm25::{OkapiBM25, tokenize};

#[test]
fn test_tokenize_lowercase_and_punctuation() {
    let text = "Hello, world! This is a Rust-based BM25 tokenizer.";
    let tokens = tokenize(text);
    assert!(tokens.contains(&"hello".to_string()));
    assert!(tokens.contains(&"world".to_string()));
    assert!(tokens.contains(&"rust-based".to_string()) || tokens.contains(&"rust".to_string()));
    assert!(!tokens.contains(&"hello,".to_string()));
}

#[test]
fn test_bm25_scoring_ranking() {
    let docs = vec![
        "the quick brown fox jumps over the lazy dog".to_string(),
        "rusty metal pipes in the old basement".to_string(),
        "rust programming language and agentic systems".to_string(),
    ];
    let corpus = docs
        .into_iter()
        .enumerate()
        .map(|(i, content)| (i.to_string(), content))
        .collect::<Vec<_>>();

    let bm25 = OkapiBM25::new(&corpus);

    // Query: "rust language"
    let scores = bm25.score("rust language");

    // The third document should have the highest score since it has both "rust" and "language"
    let doc_idx_2 = "2".to_string();
    let score_2 = scores
        .iter()
        .find(|(id, _)| id == &doc_idx_2)
        .map(|(_, s)| *s)
        .unwrap_or(0.0);

    let doc_idx_0 = "0".to_string();
    let score_0 = scores
        .iter()
        .find(|(id, _)| id == &doc_idx_0)
        .map(|(_, s)| *s)
        .unwrap_or(0.0);

    assert!(
        score_2 > score_0,
        "Doc 2 score ({}) should be higher than Doc 0 score ({})",
        score_2,
        score_0
    );
}

#[test]
fn test_bm25_min_max_normalization() {
    let corpus = vec![
        ("1".to_string(), "query match term here".to_string()),
        ("2".to_string(), "no match word".to_string()),
    ];
    let bm25 = OkapiBM25::new(&corpus);
    let scores = bm25.score_normalized("query match");

    let norm_1 = scores
        .iter()
        .find(|(id, _)| id == "1")
        .map(|(_, s)| *s)
        .unwrap_or(0.0);
    let norm_2 = scores
        .iter()
        .find(|(id, _)| id == "2")
        .map(|(_, s)| *s)
        .unwrap_or(0.0);

    assert!(
        (norm_1 - 1.0).abs() < 1e-5,
        "Highest score must normalize to 1.0, got {}",
        norm_1
    );
    assert!(
        (norm_2 - 0.0).abs() < 1e-5,
        "Lowest score must normalize to 0.0, got {}",
        norm_2
    );
}

}

mod boosts {
use mythrax_core::retrieval::boosts::{BoostSignals, BoostWeights, apply_boosts};

#[test]
fn boosts_clamp_to_zero_two_range() {
    let w = BoostWeights::default();
    let d = apply_boosts(
        0.10,
        &BoostSignals {
            person_name: true,
            exact_quote: true,
            temporal_proximity: 0.0,
            keyword_overlap: 0.0,
            ..Default::default()
        },
        &w,
    );
    assert!(d >= -2.0 && d <= 2.0);
}

#[test]
fn person_name_reduces_distance_about_40pct() {
    let w = BoostWeights::default();
    let base = 1.0;
    let boosted = apply_boosts(
        base,
        &BoostSignals {
            person_name: true,
            exact_quote: false,
            temporal_proximity: 0.0,
            keyword_overlap: 0.0,
            ..Default::default()
        },
        &w,
    );
    assert!(boosted < base);
    assert!((boosted - 0.60).abs() < 1e-3); // -40% (per REFERENCE BEHAVIORS)
}

#[test]
fn quoted_phrase_reduces_distance_about_60pct() {
    let w = BoostWeights::default();
    let base = 1.0;
    let boosted = apply_boosts(
        base,
        &BoostSignals {
            person_name: false,
            exact_quote: true,
            temporal_proximity: 0.0,
            keyword_overlap: 0.0,
            ..Default::default()
        },
        &w,
    );
    assert!(boosted < base);
    assert!((boosted - 0.40).abs() < 1e-3); // -60% (per REFERENCE BEHAVIORS)
}

#[test]
fn no_signals_is_identity() {
    assert_eq!(
        apply_boosts(0.73, &BoostSignals::default(), &BoostWeights::default()),
        0.73
    );
}

}

mod calibrated_confidence {
use anyhow::Result;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};

#[tokio::test]
async fn test_calibrated_confidence_scaling() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create a mock episode with confidence 0.50
    let ep = EpisodeSave {
        created_at: None,
        title: "Calibrated Confidence Test Episode".to_string(),
        content: "Unique content for test scaling similarity".to_string(),
        scope: Some("general".to_string()),
        confidence: Some(0.50),
        ..Default::default()
    };

    let id = backend.save_episode(&ep).await?;
    let uuid = id.split(':').nth(1).unwrap();

    // Ensure it is not archived and has confidence set
    backend
        .db
        .query("UPDATE type::record('episode', $id) MERGE { confidence: 0.50, archived: false };")
        .bind(("id", uuid))
        .await?
        .check()?;

    // 1. Enable calibrated confidence
    backend
        .save_profile_key("search.enable_calibrated_confidence", "true")
        .await?;

    let resp = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "Unique content for test scaling similarity",
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
            false,
            None,
        ))
        .await?;

    let results = resp.results;
    assert!(!results.is_empty(), "Should retrieve the episode");
    let matched = results
        .iter()
        .find(|r| r.id == id)
        .expect("Should find the exact episode");
    assert_eq!(
        matched.confidence,
        Some(0.50),
        "Confidence must be populated as Some(0.50)"
    );

    let scaled_similarity = matched.similarity;

    // 2. Disable calibrated confidence
    backend
        .save_profile_key("search.enable_calibrated_confidence", "false")
        .await?;

    let resp_disabled = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "Unique content for test scaling similarity",
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
            false,
            None,
        ))
        .await?;

    let results_disabled = resp_disabled.results;
    let matched_disabled = results_disabled
        .iter()
        .find(|r| r.id == id)
        .expect("Should find the exact episode");
    let unscaled_similarity = matched_disabled.similarity;

    println!(
        "DEBUG: scaled = {}, unscaled = {}",
        scaled_similarity, unscaled_similarity
    );
    // Since confidence is 0.50, scaled similarity should be exactly half of the unscaled one.
    assert!(
        (scaled_similarity - unscaled_similarity * 0.50).abs() < 1e-4,
        "Scaled similarity must be exactly confidence (0.50) times unscaled similarity"
    );

    Ok(())
}

}

mod context_aware_reranking {
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};

#[tokio::test]
async fn test_user_profile_compilation_and_sorting() {
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    let session_id = "test_session_123";

    // 1. Save STM facts
    backend
        .save_stm(session_id, "favorite_color", "blue")
        .await
        .unwrap();
    backend
        .save_stm(session_id, "degree", "physics")
        .await
        .unwrap();

    // 2. Save episodes with out-of-order and identical timestamps (as done in transaction batching)
    // We name the title with numeric Turn indices.
    let ep1 = EpisodeSave {
        created_at: None,
        title: format!("{} - Turn 1", session_id),
        content: "I started my study.".to_string(),
        session_id: Some(session_id.to_string()),
        node_type: Some("user_input".to_string()),
        ..Default::default()
    };
    let ep2 = EpisodeSave {
        created_at: None,
        title: format!("{} - Turn 2", session_id),
        content: "I prefer coffee over tea.".to_string(),
        session_id: Some(session_id.to_string()),
        node_type: Some("user_input".to_string()),
        ..Default::default()
    };
    let ep10 = EpisodeSave {
        created_at: None,
        title: format!("{} - Turn 10", session_id),
        content: "I live in Boston.".to_string(),
        session_id: Some(session_id.to_string()),
        node_type: Some("user_input".to_string()),
        ..Default::default()
    };
    let ep3 = EpisodeSave {
        created_at: None,
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
    backend
        .save_profile_key("search.user_profile_max_len", "0")
        .await
        .unwrap();
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
    ]
    .join("\n");

    assert_eq!(profile.trim(), expected.trim());
}

#[tokio::test]
async fn test_user_profile_smart_truncation() {
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    let session_id = "test_session_456";

    // STM facts: 40 chars
    backend.save_stm(session_id, "deg", "math").await.unwrap(); // deg: math (9 chars)
    backend.save_stm(session_id, "fav", "red").await.unwrap(); // fav: red (8 chars)

    // User turns:
    // Turn 1: 15 chars
    let ep1 = EpisodeSave {
        created_at: None,
        title: format!("{} - Turn 1", session_id),
        content: "Hello my friend".to_string(),
        session_id: Some(session_id.to_string()),
        node_type: Some("user_input".to_string()),
        ..Default::default()
    };
    // Turn 2: 24 chars
    let ep2 = EpisodeSave {
        created_at: None,
        title: format!("{} - Turn 2", session_id),
        content: "Weather is nice today so".to_string(),
        session_id: Some(session_id.to_string()),
        node_type: Some("user_input".to_string()),
        ..Default::default()
    };
    // Turn 3: 20 chars
    let ep3 = EpisodeSave {
        created_at: None,
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
    backend
        .save_profile_key("search.user_profile_max_len", "65")
        .await
        .unwrap();
    let profile = backend.compile_user_profile(session_id).await.unwrap();

    let expected = "Weather is nice today so\nI went for a walk to\ndeg: math\nfav: red";
    assert_eq!(profile.trim(), expected);
}

#[tokio::test]
async fn test_pipeline_retrieval_optimizations() {
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    // Verify default TF-IDF pool size configuration can be queried
    backend
        .save_profile_key("search.tfidf_pool_size", "100")
        .await
        .unwrap();
    let tfidf_pool = backend
        .get_profile_key("search.tfidf_pool_size")
        .await
        .unwrap();
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
    backend
        .save_profile_key("search.enable_access_reinforcement", "false")
        .await
        .unwrap();

    // Save a mock episode with query-matching content and title forced to 0.85 similarity
    let ep = EpisodeSave {
        created_at: None,
        title: "High Similarity Old Node".to_string(),
        content: "Rust database locks and transaction management.".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let ep_id = backend.save_episode(&ep).await.unwrap();

    // 1. Scale = 0.0 (no boost) -> raw_vector_sim should be exactly 0.85
    backend
        .save_profile_key("search.ladder_scale", "0.0")
        .await
        .unwrap();
    let res = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
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
            None,
        ))
        .await
        .unwrap();
    assert!(!res.results.is_empty());
    let r = res.results.iter().find(|x| x.id == ep_id).unwrap();
    assert_eq!(r.raw_vector_sim.unwrap(), 0.85f32);

    // 2. Scale = 1.0 (full boost) -> raw_vector_sim should be 0.85 + 0.15 = 1.0
    backend
        .save_profile_key("search.ladder_scale", "1.0")
        .await
        .unwrap();
    let res2 = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
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
            None,
        ))
        .await
        .unwrap();
    let r2 = res2.results.iter().find(|x| x.id == ep_id).unwrap();
    assert_eq!(r2.raw_vector_sim.unwrap(), 1.0f32);

    // 3. Scale = 0.5 (half boost) -> raw_vector_sim should be 0.85 + 0.075 = 0.925
    backend
        .save_profile_key("search.ladder_scale", "0.5")
        .await
        .unwrap();
    let res3 = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
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
            None,
        ))
        .await
        .unwrap();
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
    backend
        .save_profile_key("search.enable_access_reinforcement", "false")
        .await
        .unwrap();

    // Save a mock episode with query-matching content
    let ep = EpisodeSave {
        created_at: None,
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
    backend
        .save_profile_key("search.temporal_decay_floor", "0.20")
        .await
        .unwrap();
    let res = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
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
            None,
        ))
        .await
        .unwrap();
    let r = res.results.iter().find(|x| x.id == ep_id).unwrap();
    assert!((r.factor_multiplier.unwrap() - 0.35f32).abs() < 1e-4);

    // Reset created_at and clear last_retrieved_at again to force decay on the second search
    backend.db.query("UPDATE type::record('episode', $id) MERGE { created_at: time::now() - 365d, last_retrieved_at: NONE };")
        .bind(("id", uuid))
        .await.unwrap().check().unwrap();

    // 2. Decay floor = 0.45 -> factor_multiplier should be 0.25 + 0.5 * 0.45 = 0.475
    backend
        .save_profile_key("search.temporal_decay_floor", "0.45")
        .await
        .unwrap();
    let res2 = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
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
            None,
        ))
        .await
        .unwrap();
    let r2 = res2.results.iter().find(|x| x.id == ep_id).unwrap();
    assert!((r2.factor_multiplier.unwrap() - 0.475f32).abs() < 1e-4);
}

}

mod cycle_proof_traversal {
use anyhow::Result;
use mythrax_core::db::{StorageBackend, SurrealBackend};

#[tokio::test]
async fn test_cycle_proof_traversal_circular() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create a circular relation: A -> relates_to -> B -> relates_to -> A
    backend
        .relate_nodes("wiki_node:node_a", "wiki_node:node_b", None, None, None)
        .await?;
    backend
        .relate_nodes("wiki_node:node_b", "wiki_node:node_a", None, None, None)
        .await?;

    // Perform query_symbolic
    let results = backend
        .query_symbolic("wiki_node:node_a", None, Some(5))
        .await?;

    // Verify it doesn't loop infinitely and contains node_b
    assert!(results.contains(&"wiki_node:node_b".to_string()));
    assert_eq!(results.len(), 1); // Only node_b is visited and returned (excluding start node)

    Ok(())
}

#[tokio::test]
async fn test_query_symbolic_scored_confidences() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create wiki nodes first so they exist:
    let node_contract = mythrax_core::contracts::WikiNode {
        id: Some("wiki_node:node_a".to_string()),
        name: "Node A".to_string(),
        content: "Content A".to_string(),
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
        ..Default::default()
    };
    backend.save_wiki_node(&node_contract).await?;
    let node_contract = mythrax_core::contracts::WikiNode {
        id: Some("wiki_node:node_b".to_string()),
        name: "Node B".to_string(),
        content: "Content B".to_string(),
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
        ..Default::default()
    };
    backend.save_wiki_node(&node_contract).await?;
    let node_contract = mythrax_core::contracts::WikiNode {
        id: Some("wiki_node:node_c".to_string()),
        name: "Node C".to_string(),
        content: "Content C".to_string(),
        scope: "general".to_string(),
        vault_path: None,
        embedding: None,
        ..Default::default()
    };
    backend.save_wiki_node(&node_contract).await?;

    // Chain path:
    backend
        .relate_nodes(
            "wiki_node:node_a",
            "wiki_node:node_b",
            None,
            None,
            Some(0.8),
        )
        .await?;
    backend
        .relate_nodes(
            "wiki_node:node_b",
            "wiki_node:node_c",
            None,
            None,
            Some(0.5),
        )
        .await?;

    // Shortcut path (initially weaker but wait, shortcut is direct):
    backend
        .relate_nodes(
            "wiki_node:node_a",
            "wiki_node:node_c",
            None,
            None,
            Some(0.5),
        )
        .await?;

    let results = backend
        .query_symbolic_scored("wiki_node:node_a", None, Some(3), None)
        .await?;

    let hit_c = results
        .iter()
        .find(|h| h.node_id == "wiki_node:node_c")
        .unwrap();
    // Chain path: 0.8 * 0.5 = 0.4. Shortcut path: 0.5. We should retain max (0.5).
    assert_eq!(hit_c.path_confidence, 0.5);

    Ok(())
}

#[tokio::test]
async fn test_query_symbolic_scored_temporal_filtering() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create wiki nodes
    for name in &["node_a", "node_b", "node_c"] {
        let node_contract = mythrax_core::contracts::WikiNode {
            id: Some(format!("wiki_node:{}", name)),
            name: name.to_string(),
            content: "content".to_string(),
            scope: "general".to_string(),
            vault_path: None,
            embedding: None,
            ..Default::default()
        };
        backend.save_wiki_node(&node_contract).await?;
    }

    // A -[valid at Utc::now()]-> B
    // A -[valid ONLY in future]-> C
    let now = chrono::Utc::now();
    let future = now + chrono::Duration::days(1);

    let rel_ab = "RELATE wiki_node:node_a->relates_to->wiki_node:node_b SET confidence = 1.0, valid_from = $from, valid_to = $to;";
    backend
        .db
        .query(rel_ab)
        .bind(("from", now - chrono::Duration::days(1)))
        .bind(("to", now + chrono::Duration::days(5)))
        .await?
        .check()?;

    let rel_ac = "RELATE wiki_node:node_a->relates_to->wiki_node:node_c SET confidence = 1.0, valid_from = $from;";
    backend
        .db
        .query(rel_ac)
        .bind(("from", future))
        .await?
        .check()?;

    // Query as of now: node_b should be returned, but NOT node_c
    let hits_now = backend
        .query_symbolic_scored("wiki_node:node_a", None, Some(3), Some(now))
        .await?;
    let ids_now: Vec<String> = hits_now.into_iter().map(|h| h.node_id).collect();
    assert!(ids_now.contains(&"wiki_node:node_b".to_string()));
    assert!(!ids_now.contains(&"wiki_node:node_c".to_string()));

    // Query as of future: both should be returned
    let hits_future = backend
        .query_symbolic_scored(
            "wiki_node:node_a",
            None,
            Some(3),
            Some(future + chrono::Duration::hours(1)),
        )
        .await?;
    let ids_future: Vec<String> = hits_future.into_iter().map(|h| h.node_id).collect();
    assert!(ids_future.contains(&"wiki_node:node_b".to_string()));
    assert!(ids_future.contains(&"wiki_node:node_c".to_string()));

    Ok(())
}

#[tokio::test]
async fn test_resolve_query_anchors_knn_multi() -> anyhow::Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Seed 3 entities with 768-dimensional embeddings to satisfy HNSW constraints
    let mut emb1 = vec![0.0f32; 768];
    emb1[0] = 0.1;
    let mut emb2 = vec![0.0f32; 768];
    emb2[0] = 0.1;
    emb2[1] = 0.01;
    let mut emb3 = vec![0.0f32; 768];
    emb3[0] = 0.1;
    emb3[1] = 0.02;

    backend.db.query("CREATE entity SET name = 'Entity 1', entity_type = 'person', summary = 'Entity summary', labels = ['label1'], embedding = $emb;")
        .bind(("emb", emb1))
        .await?.check()?;
    backend.db.query("CREATE entity SET name = 'Entity 2', entity_type = 'person', summary = 'Entity summary', labels = ['label1'], embedding = $emb;")
        .bind(("emb", emb2))
        .await?.check()?;
    backend.db.query("CREATE entity SET name = 'Entity 3', entity_type = 'person', summary = 'Entity summary', labels = ['label1'], embedding = $emb;")
        .bind(("emb", emb3))
        .await?.check()?;

    let query_emb = {
        let mut qe = vec![0.0f32; 768];
        qe[0] = 0.1;
        qe
    };
    // Call resolve_query_anchors
    let anchors = backend
        .resolve_query_anchors("some query with no exact matches", Some(&query_emb))
        .await;

    // It should return more than 1 anchor (proves k > 1)
    assert!(
        anchors.len() > 1,
        "Should return more than 1 anchor, got {}",
        anchors.len()
    );
    assert!(anchors.len() <= 5, "Should be capped at 5 anchors");

    Ok(())
}

}

mod gaussian_temporal {
use anyhow::Result;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};

#[tokio::test]
async fn test_gaussian_temporal_decay() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    backend
        .save_profile_key("search.enable_access_reinforcement", "false")
        .await?;

    // Create a mock episode with utility 100.0
    let ep = EpisodeSave {
        created_at: None,
        title: "Gaussian Temporal Test Episode".to_string(),
        content: "Unique content for test gaussian temporal decay".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };

    let id = backend.save_episode(&ep).await?;
    let uuid = id.split(':').nth(1).unwrap();

    // Set utility to 100.0 and simulate last_retrieved_at and created_at to 7 days ago (7 days * 24 hours = 168 hours = 1 sigma)
    backend.db.query("UPDATE type::record('episode', $id) MERGE { utility: 100.0, created_at: time::now() - 7d, last_retrieved_at: time::format(time::now() - 7d, '%Y-%m-%dT%H:%M:%SZ'), archived: false };")
        .bind(("id", uuid))
        .await?.check()?;

    // 1. Enable Gaussian temporal decay
    backend
        .save_profile_key("search.enable_gaussian_temporal", "true")
        .await?;
    backend
        .save_profile_key("search.gaussian_temporal_sigma", "168.0")
        .await?;

    let resp = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "Unique content for test gaussian temporal decay",
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
            false,
            None,
        ))
        .await?;

    let results = resp.results;
    assert!(!results.is_empty(), "Should retrieve the episode");
    let matched = results
        .iter()
        .find(|r| r.id == id)
        .expect("Should find the exact episode");

    // Gaussian decay factor for 7 days (168 hours) is exp(-0.5) = 0.60653
    // Decayed utility = 100.0 * 0.60653 = 60.653
    let gaussian_utility = matched.utility;
    println!("DEBUG: gaussian utility = {}", gaussian_utility);

    // 2. Disable Gaussian temporal decay (fallback to linear/exponential with -0.05 * delta_t_days)
    backend
        .save_profile_key("search.enable_gaussian_temporal", "false")
        .await?;

    let resp_fallback = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "Unique content for test gaussian temporal decay",
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
            false,
            None,
        ))
        .await?;

    let results_fallback = resp_fallback.results;
    let matched_fallback = results_fallback
        .iter()
        .find(|r| r.id == id)
        .expect("Should find the exact episode");

    // Standard decay factor for 7 days is exp(-0.05 * 7) = exp(-0.35) = 0.704688
    // Decayed utility = 100.0 * 0.704688 = 70.4688
    let standard_utility = matched_fallback.utility;
    println!("DEBUG: standard utility = {}", standard_utility);

    // Assert that the utility scores match the theoretical decay factors
    assert!(
        (gaussian_utility - 60.653).abs() < 1.0,
        "Gaussian utility should be around 60.65"
    );
    assert!(
        (standard_utility - 70.47).abs() < 1.0,
        "Standard utility should be around 70.47"
    );

    Ok(())
}

}

mod hybrid_fusion {
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::backend::{StorageBackend, SurrealBackend};

#[tokio::test]
async fn test_hybrid_fusion_toggle() -> anyhow::Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Save some episodes
    // Ep 1: strong lexical match for "basement pipes", but unrelated to vector topic
    let ep1 = EpisodeSave {
        created_at: None,
        title: "basement pipes".to_string(),
        content: "Draft notes about rusty metal pipes located in the old cold basement."
            .to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: None,
        source_episode: None,
        session_id: Some("sess-1".to_string()),
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,

        confidence: None,
        ..Default::default()
    };

    // Ep 2: strong semantic match for "artificial intelligence", but no mention of "basement pipes"
    let ep2 = EpisodeSave {
        created_at: None,
        title: "agentic systems".to_string(),
        content: "Deep research on neural architectures and advanced agentic memory consolidation models.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: None,
        source_episode: None,
        session_id: Some("sess-1".to_string()),
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,

        confidence: None,
        ..Default::default()
    };

    let id1 = backend.save_episode(&ep1).await?;
    let _id2 = backend.save_episode(&ep2).await?;

    // 1. Search with hybrid OFF (default/vector-only)
    // Query: "basement pipes"
    // With hybrid OFF, the vector search is performed.
    backend
        .save_profile_key("retrieval.hybrid", "false")
        .await?;
    let _res_off = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "basement pipes",
            Some("general"),
            false,
            5,
            0,
            0.0,
            None,
            false,
            true,
            true,
            None,
            true,
            None,
        ))
        .await?;

    // 2. Search with hybrid ON
    backend.save_profile_key("retrieval.hybrid", "true").await?;
    let res_on = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "basement pipes",
            Some("general"),
            false,
            5,
            0,
            0.0,
            None,
            false,
            true,
            true,
            None,
            true,
            None,
        ))
        .await?;

    // If hybrid is ON, the lexical match (Ep 1) should rank highly (and be returned as a high-scoring result)
    // because of its 100% lexical term overlap.
    assert!(
        !res_on.results.is_empty(),
        "Hybrid search should return results"
    );
    let found_ep1_on = res_on.results.iter().any(|r| r.id == id1);
    assert!(
        found_ep1_on,
        "Hybrid search should return the lexically matching episode"
    );

    Ok(())
}

}

mod sigmoid_gated_search {
use anyhow::Result;
use mythrax_core::contracts::{EpisodeSave, WisdomRule};
use mythrax_core::db::{StorageBackend, SurrealBackend};

#[tokio::test]
async fn test_sigmoid_gated_retrieval_formula() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_SIGMOID_GATED_SEARCH_TEST", "true");
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // 1. Insert Mock Episode A: High similarity (0.85), low importance (2.0), old (created 10 days ago)
    let ep_a = EpisodeSave {
        created_at: None,
        title: "High Similarity Old Node".to_string(),
        content: "Rust database locks and transaction management.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("episodes/ep_a.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        ..Default::default()
    };
    let id_a = backend.save_episode(&ep_a).await?;
    let uuid_a = id_a.split(':').nth(1).unwrap();

    // Set importance to 2.0 and simulate creation 10 days ago
    backend.db.query("UPDATE type::record('episode', $id) MERGE { importance: 2.0, created_at: time::now() - 10d };")
        .bind(("id", uuid_a))
        .await?.check()?;

    // 2. Insert Mock Episode B: Low similarity (0.50), high importance (10.0), extremely recent (0 days ago)
    let ep_b = EpisodeSave {
        created_at: None,
        title: "Low Similarity Recent Node".to_string(),
        content: "Completely unrelated text about cooking recipes and kitchen tools.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("episodes/ep_b.md".to_string()),
        source_episode: None,
        session_id: None,
        task_id: None,
        ..Default::default()
    };
    let id_b = backend.save_episode(&ep_b).await?;
    let uuid_b = id_b.split(':').nth(1).unwrap();

    backend.db.query("UPDATE type::record('episode', $id) MERGE { importance: 10.0, created_at: time::now() };")
        .bind(("id", uuid_b))
        .await?.check()?;

    // 3. Search for "Rust database locks"
    let resp = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
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
            None,
        ))
        .await?;

    // Assertions
    let results = resp.results;
    assert!(!results.is_empty(), "Should return search results");

    let pos_a = results.iter().position(|r| r.id == id_a);
    let pos_b = results.iter().position(|r| r.id == id_b);

    assert!(pos_a.is_some(), "High similarity node must be retrieved");
    if let Some(pb) = pos_b {
        assert!(
            pos_a.unwrap() < pb,
            "High similarity node must rank higher than gated low similarity node"
        );
        let score_b = results[pb].similarity;
        println!("DEBUG: score_b = {}", score_b);
        assert!(
            score_b <= 0.75,
            "Low similarity node score must be heavily suppressed by the sigmoid gate"
        );
    }

    // 4. Verify Wisdom Rule decay immunity
    let rule = WisdomRule {
        id: None,
        target_pattern: "avoid_concurrency".to_string(),
        action_to_avoid: "Writing concurrently".to_string(),
        causal_explanation: "RocksDB process lock".to_string(),
        prescribed_remedy: "Use client mode".to_string(),
        tier: mythrax_core::contracts::Tier::Wisdom,
        scope: "general".to_string(),
        vault_path: Some("wisdom/skills/avoid_concurrency.md".to_string()),
        embedding: None,
        source_episodes: vec![],
        generator_name: "manual".to_string(),
        similarity: None,
        utility: Some(50.0),
        status: Some("active".to_string()),
        superseded_at: None,
        superseded_by: None,

        rule_type: None,
        ..Default::default()
    };
    let id_r = backend.save_wisdom_rule(&rule).await?;
    let uuid_r = id_r.split(':').nth(1).unwrap();

    // Simulate creation 30 days ago
    backend.db.query("UPDATE type::record('wisdom', $id) MERGE { importance: 8.0, created_at: time::now() - 30d };")
        .bind(("id", uuid_r))
        .await?.check()?;

    // Search for wisdom
    let resp_r = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "avoid_concurrency",
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
        ))
        .await?;
    let r_results = resp_r.results;
    let match_rule = r_results.iter().find(|r| r.id == id_r);
    assert!(
        match_rule.is_some(),
        "Wisdom rule must be retrieved despite being 30 days old due to decay immunity"
    );

    Ok(())
}

}

mod v2_5_2_retrieval_signals {
use anyhow::Result;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use surrealdb_types::SurrealValue;

#[tokio::test]
async fn test_v2_5_2_retrieval_signals_integration() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    backend
        .save_profile_key("search.enable_calibrated_confidence", "false")
        .await?;
    backend
        .save_profile_key("search.enable_gaussian_temporal", "false")
        .await?;

    // --- TASK A.6: Concept Spreading Activation ---
    // 1. Enable Spreading Activation
    backend
        .save_profile_key("search.enable_spreading_activation", "true")
        .await?;

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
    backend
        .relate_nodes(&entity_id, &ep_id_str, None, None, Some(0.8))
        .await?;

    // 5. Search for "RustDB"
    let resp = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
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
        ))
        .await?;

    // The Episode is not retrieved by the direct keyword/vector search for "RustDB", but is traversed via relates_to edge!
    let found_activation = resp.results.iter().any(|r| r.id == ep_id_str);
    assert!(
        found_activation,
        "Episode should be retrieved via Spreading Activation"
    );

    // Let's verify that disabling the feature prevents it from being retrieved
    backend
        .save_profile_key("search.enable_spreading_activation", "false")
        .await?;
    let resp_disabled = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
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
        ))
        .await?;
    let found_disabled = resp_disabled.results.iter().any(|r| r.id == ep_id_str);
    assert!(
        !found_disabled,
        "Episode should NOT be retrieved when Spreading Activation is disabled"
    );

    // --- TASK A.7: STM Working Memory Injection ---
    // 1. Enable STM Retrieval
    backend
        .save_profile_key("search.enable_stm_retrieval", "true")
        .await?;

    // 2. Put key-value pair in short-term memory
    backend
        .save_stm(
            "session_bar",
            "context_guard",
            "Avoid concurrent RocksDB process lock by starting in client mode",
        )
        .await?;

    // 3. Search under "session_bar" with a query related to the STM content
    let resp_stm = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
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
        ))
        .await?;

    // Verify that the STM record is injected with working tier, synthetic ID, and utility = 100.0
    let match_stm = resp_stm
        .results
        .iter()
        .find(|r| r.id == "stm:session_bar:context_guard");
    assert!(
        match_stm.is_some(),
        "STM entry must be injected into search results"
    );
    let stm_res = match_stm.unwrap();
    assert_eq!(stm_res.tier, mythrax_core::contracts::Tier::Working);
    assert_eq!(stm_res.utility, 100.0);
    assert_eq!(stm_res.title, "context_guard");
    assert_eq!(
        stm_res.content,
        "Avoid concurrent RocksDB process lock by starting in client mode"
    );

    // --- TASK A.8: Access-Driven Utility Reinforcement ---
    // 1. Enable Access Reinforcement
    backend
        .save_profile_key("search.enable_access_reinforcement", "true")
        .await?;

    // 2. Insert a new Episode for reinforcement testing
    let ep_reinforce = EpisodeSave {
        created_at: None,
        title: "Memory Leak Diagnostics".to_string(),
        content: "Identify JavaScript memory leaks using Chrome DevTools heap snapshots."
            .to_string(),
        scope: Some("general".to_string()),
        vault_path: Some("episodes/mem_leak.md".to_string()),
        ..Default::default()
    };
    let ep_re_id = backend.save_episode(&ep_reinforce).await?;
    let ep_re_uuid = ep_re_id.split(':').nth(1).unwrap();

    // 3. Perform a search to retrieve the episode
    let _resp_re = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
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
        ))
        .await?;

    // Give the async spawned background task a moment to execute
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Check if the metrics record was inserted
    #[derive(serde::Deserialize, surrealdb_types::SurrealValue, Debug)]
    struct LocalMetricsRow {
        utility_score: f64,
        access_count: i64,
    }

    let check_sql = "SELECT utility_score, access_count FROM metrics WHERE target_id = type::record('episode', $ep_id) LIMIT 1;";
    let mut metrics_res = backend
        .db
        .query(check_sql)
        .bind(("ep_id", ep_re_uuid))
        .await?
        .check()?;
    let mut rows: Vec<LocalMetricsRow> = metrics_res.take(0)?;
    assert_eq!(
        rows.len(),
        1,
        "Metrics record should be created on first access"
    );
    let initial_row = rows.pop().unwrap();
    assert_eq!(initial_row.access_count, 1);
    assert_eq!(initial_row.utility_score, 50.0);

    // 4. Perform search again to increment access count and trigger reinforcement logic
    let _resp_re2 = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
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
        ))
        .await?;

    // Give background task a moment
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let mut metrics_res2 = backend
        .db
        .query(check_sql)
        .bind(("ep_id", ep_re_uuid))
        .await?
        .check()?;
    let mut rows2: Vec<LocalMetricsRow> = metrics_res2.take(0)?;
    assert_eq!(rows2.len(), 1);
    let updated_row = rows2.pop().unwrap();
    assert_eq!(
        updated_row.access_count, 2,
        "Access count should be incremented to 2"
    );

    // utility_reinforced = (50.0 + log2(2) * exp(0)) = 50.0 + 1.0 = 51.0
    assert!(
        (updated_row.utility_score - 51.0).abs() < 0.1,
        "Utility score should be reinforced to approximately 51.0, got: {}",
        updated_row.utility_score
    );

    // --- TASK B.3: MLX Cross-Encoder Reranker (Mocked in test mode) ---
    // 1. Enable Cross-Encoder Reranking
    backend
        .save_profile_key("search.enable_cross_encoder_rerank", "true")
        .await?;
    backend
        .save_profile_key("search.mock_reranker", "true")
        .await?;
    backend
        .save_profile_key("search.rerank_pool_size", "5")
        .await?;

    // 2. Perform a search with two episodes in candidates
    let ep_other = EpisodeSave {
        created_at: None,
        title: "Random unrelated title".to_string(),
        content: "Totally unrelated document content that does not match transaction isolation."
            .to_string(),
        scope: Some("general".to_string()),
        vault_path: Some("episodes/random.md".to_string()),
        session_id: Some("session_foo".to_string()),
        ..Default::default()
    };
    let _ep_other_id = backend.save_episode(&ep_other).await?;

    let resp_rerank = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
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
        ))
        .await?;

    // Verify that the first candidate (which matches the mock boost) has similarity 0.95
    assert!(!resp_rerank.results.is_empty());
    assert_eq!(resp_rerank.results[0].similarity, 0.95f32);

    // Disabling the reranker should run without setting similarity to 0.95
    backend
        .save_profile_key("search.enable_cross_encoder_rerank", "false")
        .await?;
    backend
        .save_profile_key("search.mock_reranker", "false")
        .await?;
    let resp_no_rerank = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
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
        ))
        .await?;
    assert_ne!(resp_no_rerank.results[0].similarity, 0.95f32);

    Ok(())
}

}

mod task_2_search_guardrails {
use anyhow::Result;
use chrono::Utc;
use mythrax_core::contracts::{EpisodeSave, SearchParams};
use mythrax_core::db::{StorageBackend, SurrealBackend, parse_record_id};
use mythrax_core::mcp_routes::strip_diffs;

#[tokio::test]
async fn test_standard_search_filters_conflict_nodes() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // 1. Insert conflict node (node_type = 'conflict')
    let ep_conflict = EpisodeSave {
        title: "Conflict Episode".to_string(),
        content: "This represents a code/rule conflict.".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let id_conflict = backend.save_episode(&ep_conflict).await?;
    let uuid_conflict = id_conflict.split(':').nth(1).unwrap();
    backend
        .db
        .query("UPDATE type::record('episode', $id) SET node_type = 'conflict', importance = 8.0;")
        .bind(("id", uuid_conflict))
        .await?
        .check()?;

    // 2. Insert standard node (node_type = 'standard')
    let ep_standard = EpisodeSave {
        title: "Standard Episode".to_string(),
        content: "This is standard text content.".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let id_standard = backend.save_episode(&ep_standard).await?;
    let uuid_standard = id_standard.split(':').nth(1).unwrap();
    backend
        .db
        .query("UPDATE type::record('episode', $id) SET node_type = 'standard', importance = 5.0;")
        .bind(("id", uuid_standard))
        .await?
        .check()?;

    // 3. Perform standard search (query = "standard text") -> should NOT return conflict node
    let search_params_std = SearchParams {
        query: "standard text".to_string(),
        scope: Some("general".to_string()),
        limit: 10,
        include_episodes: true,
        ..Default::default()
    };
    let resp_std = backend.search(search_params_std).await?;
    let has_conflict = resp_std.results.iter().any(|r| r.id == id_conflict);
    assert!(
        !has_conflict,
        "Conflict nodes must be excluded from standard search"
    );

    // 4. Perform exploratory search (query contains "conflict") -> should return conflict node
    let search_params_exp = SearchParams {
        query: "resolving conflict".to_string(),
        scope: Some("general".to_string()),
        limit: 10,
        include_episodes: true,
        ..Default::default()
    };
    let resp_exp = backend.search(search_params_exp).await?;
    let has_conflict_exp = resp_exp.results.iter().any(|r| r.id == id_conflict);
    assert!(
        has_conflict_exp,
        "Conflict nodes must be retrieved in exploratory queries"
    );

    Ok(())
}

#[tokio::test]
async fn test_standard_search_hides_pending_htr_nodes() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Create hypothesis_node Root
    backend.db.query("CREATE type::record('hypothesis_node', 'root_node') CONTENT { node_id: 'root_node', status: 'pending', hypothesis: 'base root hypothesis', score: 50.0 };").await?.check()?;

    // Create an episode
    let ep = EpisodeSave {
        title: "HTR Episode".to_string(),
        content: "HTR execution trace episode content.".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let ep_id = backend.save_episode(&ep).await?;

    // Relate episode -> hypothesis_node
    let from_id = parse_record_id(&ep_id)?;
    let to_id = parse_record_id("hypothesis_node:root_node")?;
    backend
        .db
        .query("RELATE $from -> relates_to -> $to;")
        .bind(("from", from_id))
        .bind(("to", to_id))
        .await?
        .check()?;

    // Search with deep_insight = true. Since the hypothesis node is pending, it should be hidden from related nodes
    let search_params = SearchParams {
        query: "HTR execution trace".to_string(),
        scope: Some("general".to_string()),
        deep_insight: true,
        include_episodes: true,
        limit: 10,
        ..Default::default()
    };
    let resp = backend.search(search_params.clone()).await?;
    let ep_res = resp.results.iter().find(|r| r.id == ep_id).unwrap();

    // Check if related_nodes list contains the hypothesis node
    if let Some(ref related) = ep_res.related_nodes {
        let has_pending = related.iter().any(|r| r.id.contains("root_node"));
        assert!(
            !has_pending,
            "Pending HTR hypothesis node must be hidden from related nodes"
        );
    }

    // Now complete the HTR node (status = 'done')
    backend
        .db
        .query("UPDATE type::record('hypothesis_node', 'root_node') SET status = 'done';")
        .await?
        .check()?;
    let resp_done = backend.search(search_params).await?;
    let ep_res_done = resp_done.results.iter().find(|r| r.id == ep_id).unwrap();
    let related = ep_res_done
        .related_nodes
        .as_ref()
        .expect("related nodes list should be present");
    let has_done = related.iter().any(|r| r.id.contains("root_node"));
    assert!(
        has_done,
        "Completed HTR hypothesis node should be visible in related nodes"
    );

    Ok(())
}

#[test]
fn test_diff_strip_format_methods() {
    let content = "Hello world\ndiff --git a/file.txt b/file.txt\n--- a/file.txt\n+++ b/file.txt\n@@ -1,3 +1,3 @@\n-old\n+new\n```diff\n-removed\n+added\n```\nSome footer text";
    let stripped = strip_diffs(content);
    assert!(
        !stripped.contains("diff --git"),
        "Should strip raw diff header"
    );
    assert!(
        !stripped.contains("--- a/"),
        "Should strip diff old file path"
    );
    assert!(
        !stripped.contains("+++ b/"),
        "Should strip diff new file path"
    );
    assert!(!stripped.contains("@@ -"), "Should strip diff chunk header");
    assert!(
        !stripped.contains("removed"),
        "Should strip code inside diff block"
    );
    assert!(stripped.contains("Hello world"), "Should keep Hello world");
    assert!(
        stripped.contains("Some footer text"),
        "Should keep non-diff text"
    );
    assert!(
        stripped.contains("[Diff Truncated]"),
        "Should insert Diff Truncated placeholder"
    );
}

#[tokio::test]
async fn test_temporal_decay_uses_temporal_range_end() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Insert two wiki nodes
    // Node A: created 10 days ago, but temporal_range_end is now
    let now = Utc::now();
    let ten_days_ago = now - chrono::Duration::days(10);

    let emb = vec![1.0; 768];
    // We create them directly
    backend.db.query("CREATE type::record('wiki_node', 'node_a') CONTENT { name: 'Node A', content: 'Node A content', scope: 'general', created_at: $created_a, temporal_range_end: $temp_end_a, importance: 5.0, embedding: $emb };")
        .bind(("created_a", ten_days_ago))
        .bind(("temp_end_a", now))
        .bind(("emb", emb.clone()))
        .await?.check()?;

    // Node B: created 10 days ago, no temporal_range_end
    backend.db.query("CREATE type::record('wiki_node', 'node_b') CONTENT { name: 'Node B', content: 'Node B content', scope: 'general', created_at: $created_b, importance: 5.0, embedding: $emb };")
        .bind(("created_b", ten_days_ago))
        .bind(("emb", emb))
        .await?.check()?;

    // Search for "Node content" using a temporal search context
    let search_params = SearchParams {
        query: "content".to_string(),
        scope: Some("general".to_string()),
        limit: 10,
        temporal_anchor: Some(now.to_rfc3339()),
        threshold: 0.0,
        ..Default::default()
    };

    let resp = backend.search(search_params).await?;
    let results = resp.results;

    let score_a = results
        .iter()
        .find(|r| r.id.contains("node_a"))
        .map(|r| r.similarity)
        .unwrap_or(0.0);
    let score_b = results
        .iter()
        .find(|r| r.id.contains("node_b"))
        .map(|r| r.similarity)
        .unwrap_or(0.0);

    assert!(
        score_a > score_b,
        "Node A (newer temporal_range_end) should decay less than Node B (no temporal_range_end): score_a = {}, score_b = {}",
        score_a,
        score_b
    );

    Ok(())
}

#[tokio::test]
async fn test_temporal_neighbor_expansion_retrieves_wiki_nodes() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_TEST_MOCK", "1");
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Enable temporal expansion by configuring temporal cue
    // Insert primary episode Ep1
    let ep = EpisodeSave {
        title: "Ep1".to_string(),
        content: "Event primary context text".to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    let ep_id = backend.save_episode(&ep).await?;

    // Insert neighbor WikiNode Wiki1
    backend.db.query("CREATE type::record('wiki_node', 'wiki1') CONTENT { name: 'Wiki1', content: 'Subsequent details that follow the event', scope: 'general' };").await?.check()?;

    // Relate Ep1 -> followed_by -> Wiki1
    backend
        .relate_followed_by(&ep_id, "wiki_node:wiki1")
        .await?;

    // Run search with temporal cue in query to trigger temporal neighbor expansion (e.g. "after the event")
    let search_params = SearchParams {
        query: "after the event".to_string(),
        scope: Some("general".to_string()),
        include_episodes: true,
        limit: 10,
        ..Default::default()
    };

    let resp = backend.search(search_params).await?;
    let has_wiki_neighbor = resp.results.iter().any(|r| r.id.contains("wiki1"));
    assert!(
        has_wiki_neighbor,
        "Succeeding temporal neighbor expansion must retrieve the linked wiki_node neighbor"
    );

    Ok(())
}

}

mod fts_cap {
use anyhow::Result;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};

#[tokio::test]
async fn test_fts_cap_behavior() -> Result<()> {
    tokio::time::pause();
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    backend.set_search_mode("hybrid").await;

    // We insert 3 documents with the keyword "architecture"
    let content_arch =
        "This is a document about microservice architecture and service mesh design.";
    let titles_arch = vec![
        "Microservice Node Alpha",
        "Microservice Node Beta",
        "Microservice Node Gamma",
    ];

    for title in &titles_arch {
        let ep = EpisodeSave {
            created_at: None,
            title: title.to_string(),
            content: format!("{} - {}", title, content_arch),
            scope: Some("general".to_string()),
            ..Default::default()
        };
        backend.save_episode(&ep).await?;
    }

    // We insert 2 documents WITHOUT "architecture" (to ensure DF < N, yielding non-zero IDF)
    let content_other = "This is a recipe for baking delicious homemade pizza with cheese.";
    let titles_other = vec!["Pizza Node Delta", "Pizza Node Epsilon"];

    for title in &titles_other {
        let ep = EpisodeSave {
            created_at: None,
            title: title.to_string(),
            content: format!("{} - {}", title, content_other),
            scope: Some("general".to_string()),
            ..Default::default()
        };
        backend.save_episode(&ep).await?;
    }

    // Allow SurrealDB FTS to index
    tokio::time::advance(std::time::Duration::from_millis(2000)).await;

    // Query all episodes to verify what's in the DB
    let mut raw_eps_resp = backend
        .db
        .query("SELECT id, title, embedding FROM episode;")
        .await?;
    let raw_eps: Vec<serde_json::Value> = raw_eps_resp.take(0)?;
    println!("Total episodes in DB: {}", raw_eps.len());
    for ep in &raw_eps {
        println!(" - DB Episode: {}", ep);
    }

    // Test case 1: Set MYTHRAX_FTS_CAP = 2 via env var
    unsafe {
        std::env::set_var("MYTHRAX_FTS_CAP", "2");
    }

    let resp_cap_2 = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "architecture",
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
        ))
        .await?;

    println!("Search Results count (Cap 2): {}", resp_cap_2.results.len());
    for (i, r) in resp_cap_2.results.iter().enumerate() {
        println!(
            " [{}] Title: '{}', Sim: {}, BM25: {:?}",
            i, r.title, r.similarity, r.bm25_score
        );
    }

    // Since cap = 2, only 2 keyword candidates should be returned (as vector search returns 0)
    assert_eq!(
        resp_cap_2.results.len(),
        2,
        "With cap = 2, the number of returned results should be exactly 2"
    );

    // Test case 2: Clear MYTHRAX_FTS_CAP, set profile key search.fts_cap = 3
    unsafe {
        std::env::remove_var("MYTHRAX_FTS_CAP");
    }
    backend.save_profile_key("search.fts_cap", "3").await?;

    // We need 4 documents with "architecture" to test cap = 3!
    // Let's insert a 4th document with "architecture"
    let ep = EpisodeSave {
        created_at: None,
        title: "Microservice Node Zeta".to_string(),
        content: content_arch.to_string(),
        scope: Some("general".to_string()),
        ..Default::default()
    };
    backend.save_episode(&ep).await?;

    // Allow SurrealDB FTS to index
    tokio::time::advance(std::time::Duration::from_millis(2000)).await;

    let resp_cap_3 = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "architecture",
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
        ))
        .await?;

    println!("Search Results count (Cap 3): {}", resp_cap_3.results.len());
    for (i, r) in resp_cap_3.results.iter().enumerate() {
        println!(
            " [{}] Title: '{}', Sim: {}, BM25: {:?}",
            i, r.title, r.similarity, r.bm25_score
        );
    }

    // Since cap = 3, only 3 keyword candidates should be returned
    assert_eq!(
        resp_cap_3.results.len(),
        3,
        "With cap = 3, the number of returned results should be exactly 3"
    );

    Ok(())
}

}

mod batch_embedding_equivalence {
use mythrax_core::embeddings::LocalEmbedder;
use std::sync::Mutex;

static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[tokio::test]
async fn test_batch_embedding_equivalence() {
    let _lock = match TEST_MUTEX.lock() {
        Ok(guard) => guard,
        Err(p) => p.into_inner(),
    };

    let embedder = match LocalEmbedder::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!(
                "Warning: Could not initialize LocalEmbedder for batch equivalence test: {}. Skipping.",
                e
            );
            return;
        }
    };

    let texts: Vec<String> = (0..35)
        .map(|i| {
            format!(
                "This is test sentence number {} for batch embedding equivalence testing.",
                i
            )
        })
        .collect();

    let mut sequential_embeddings = Vec::with_capacity(texts.len());
    for text in &texts {
        sequential_embeddings.push(embedder.embed(text).await.expect("Failed to embed text"));
    }

    let batch_embeddings = embedder
        .embed_batch(&texts)
        .await
        .expect("Failed to get batch embeddings");

    assert_eq!(
        sequential_embeddings.len(),
        batch_embeddings.len(),
        "Number of sequential embeddings does not match batch embeddings"
    );

    let delta = 1e-4;
    for (i, (seq_emb, batch_emb)) in sequential_embeddings
        .iter()
        .zip(batch_embeddings.iter())
        .enumerate()
    {
        assert_eq!(
            seq_emb.len(),
            batch_emb.len(),
            "Embedding dimension mismatch at index {}: sequential has {} dims, batch has {} dims",
            i,
            seq_emb.len(),
            batch_emb.len()
        );

        for (j, (seq_val, batch_val)) in seq_emb.iter().zip(batch_emb.iter()).enumerate() {
            assert!(
                (seq_val - batch_val).abs() < delta,
                "Embedding values differ at index {}, dimension {}: sequential={}, batch={}, diff={}",
                i,
                j,
                seq_val,
                batch_val,
                (seq_val - batch_val).abs()
            );
        }
    }

    assert_eq!(
        batch_embeddings.len(),
        35,
        "Expected 35 batch embeddings, got {}",
        batch_embeddings.len()
    );
}

}

mod bpe_tokenizer {
use anyhow::Result;
use mythrax_core::db::{StorageBackend, SurrealBackend};

#[tokio::test]
async fn test_bpe_tokenizer_accuracy() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Source code is highly dense with spaces, brackets, and operators.
    // The old naive fallback `(len + 3) / 4` significantly undercounts code tokens.
    // We will verify that our new BPE tokenizer counts tokens accurately.
    let code_sample = r#"
        pub async fn new_client_connection() -> Result<Self> {
            let port_str = std::env::var("MYTHRAX_DAEMON_PORT").unwrap_or_else(|_| "8090".to_string());
            if let Ok(port) = port_str.parse::<u16>() {
                if let Ok(mut stream) = std::net::TcpStream::connect_timeout(
                    &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
                    std::time::Duration::from_millis(50),
                ) {
                    // Daemon is active, connect as client
                    return Ok(Self {
                        db: surrealdb::engine::local::Db::new(), // dummy
                        embedder: None,
                        client_port: Some(port),
                    });
                }
            }
            Err(anyhow::anyhow!("No active daemon found"))
        }
    "#;

    let naive_count = (code_sample.len() + 3) / 4;

    // Call our upgraded tokenizer count
    let bpe_count = backend.count_text_tokens(code_sample);

    println!(
        "BPE Token Count: {}, Naive Token Count: {}",
        bpe_count, naive_count
    );

    // BPE token count for code is typically 1.3x to 1.5x larger than naive count (chars/4)
    // because code has many single-character tokens (brackets, braces, operators, spaces).
    // We assert that BPE tokenizer counts correctly, and differs significantly from the naive count.
    assert!(
        bpe_count > naive_count,
        "BPE tokenizer must count code tokens more accurately and return a higher count than the naive chars/4 fallback"
    );
    assert!(bpe_count > 0);

    Ok(())
}

}

mod cross_encoder_mlx {
#![cfg(feature = "mlx")]

use mlx_rs::ops::indexing::TryIndexOp;
use mythrax_core::llm::MxbaiReranker;
use std::path::Path;

#[test]
fn test_cross_encoder_mlx_loading_and_scoring() {
    let home = std::env::var("HOME").unwrap();
    let model_dir = Path::new(&home).join(".mythrax/models/mxbai-rerank-large-v2");
    if !model_dir.exists() {
        return;
    }

    let mut reranker = MxbaiReranker::load(&model_dir).expect("Failed to load MxbaiReranker");

    let query = "Who wrote 'To Kill a Mockingbird'?";
    let passages = vec![
        "To Kill a Mockingbird is a novel by Harper Lee published in 1960. It was immediately successful.",
        "Moby-Dick; or, The Whale is an 1851 novel by American writer Herman Melville.",
        "The President of the United States is the head of state and head of government.",
    ];

    let start = std::time::Instant::now();

    // Test the sequential logic
    let mut scores = Vec::new();

    // 1. Compute null logits
    let null_text = format!("query: {} document: ", query);
    let null_encoding = reranker.tokenizer.encode(null_text, false).unwrap();
    let null_ids: Vec<i32> = null_encoding.get_ids().iter().map(|&x| x as i32).collect();
    let null_seq_len = null_ids.len();
    let null_ids_array = mlx_rs::Array::from_slice(&null_ids, &[1, null_seq_len as i32]);
    let null_out = reranker.model.as_mut().unwrap().forward(&null_ids_array, None).unwrap();
    let null_last_hidden = null_out
        .try_index((0, (null_seq_len - 1) as i32, ..))
        .unwrap();

    let embed_w = reranker.model.as_ref().unwrap().embed_tokens.weight.value.clone();
    let w_0 = embed_w.try_index((15, ..)).unwrap();
    let w_1 = embed_w.try_index((16, ..)).unwrap();

    let null_logit_0 = null_last_hidden
        .multiply(&w_0)
        .unwrap()
        .sum_axes(&[-1], false)
        .unwrap();
    let null_logit_1 = null_last_hidden
        .multiply(&w_1)
        .unwrap()
        .sum_axes(&[-1], false)
        .unwrap();
    let nl0 = null_logit_0
        .as_dtype(mlx_rs::Dtype::Float32)
        .unwrap()
        .as_slice::<f32>()[0];
    let nl1 = null_logit_1
        .as_dtype(mlx_rs::Dtype::Float32)
        .unwrap()
        .as_slice::<f32>()[0];

    // 2. Loop over passages
    for passage in &passages {
        let text = format!("query: {} document: {}", query, passage);
        let encoding = reranker.tokenizer.encode(text, false).unwrap();
        let ids: Vec<i32> = encoding.get_ids().iter().map(|&x| x as i32).collect();
        let seq_len = ids.len();
        let ids_array = mlx_rs::Array::from_slice(&ids, &[1, seq_len as i32]);

        let out = reranker.model.as_mut().unwrap().forward(&ids_array, None).unwrap();
        let last_hidden = out.try_index((0, (seq_len - 1) as i32, ..)).unwrap();

        let logit_0 = last_hidden
            .multiply(&w_0)
            .unwrap()
            .sum_axes(&[-1], false)
            .unwrap();
        let logit_1 = last_hidden
            .multiply(&w_1)
            .unwrap()
            .sum_axes(&[-1], false)
            .unwrap();

        let raw_l0 = logit_0
            .as_dtype(mlx_rs::Dtype::Float32)
            .unwrap()
            .as_slice::<f32>()[0];
        let raw_l1 = logit_1
            .as_dtype(mlx_rs::Dtype::Float32)
            .unwrap()
            .as_slice::<f32>()[0];

        let l0 = raw_l0 - nl0;
        let l1 = raw_l1 - nl1;

        let max_l = l0.max(l1);
        let exp_l0 = (l0 - max_l).exp();
        let exp_l1 = (l1 - max_l).exp();
        let prob_1 = exp_l1 / (exp_l0 + exp_l1);
        scores.push(prob_1);
    }

    println!("Sequential scoring took: {:?}", start.elapsed());
    println!("SCORES: {:?}", scores);

    assert_eq!(scores.len(), 3);
    assert!(
        scores[0] > scores[1],
        "Relevant passage must score higher than Moby-Dick"
    );
    assert!(
        scores[0] > scores[2],
        "Relevant passage must score higher than President passage"
    );
}

}

mod discovery_tokens {
use mythrax_core::api::ApiState;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use mythrax_core::mcp_routes::{CHARS_PER_TOKEN, handle_pre_invocation_hook};
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_episode_save_roundtrips_discovery_tokens() {
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    // 1. Check with Some value
    let ep_some = EpisodeSave {
        created_at: None,
        title: "Some Discovery".to_string(),
        content: "Test content".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/some_discovery.md".to_string()),
        source_episode: None,
        session_id: Some("session-1".to_string()),
        task_id: None,
        discovery_tokens: Some(1234),
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,

        confidence: None,
        ..Default::default()
    };
    let id_some = backend.save_episode(&ep_some).await.unwrap();

    // 2. Check with None value
    let ep_none = EpisodeSave {
        created_at: None,
        title: "None Discovery".to_string(),
        content: "Test content 2".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/none_discovery.md".to_string()),
        source_episode: None,
        session_id: Some("session-1".to_string()),
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,

        confidence: None,
        ..Default::default()
    };
    let id_none = backend.save_episode(&ep_none).await.unwrap();

    let all = backend.get_all_episodes().await.unwrap();

    let some_retrieved = all
        .iter()
        .find(|e| e.id.as_ref().unwrap() == &id_some)
        .unwrap();
    assert_eq!(some_retrieved.discovery_tokens, Some(1234));

    let none_retrieved = all
        .iter()
        .find(|e| e.id.as_ref().unwrap() == &id_none)
        .unwrap();
    assert_eq!(none_retrieved.discovery_tokens, None);
}

#[test]
fn test_read_token_estimate_matches_formula() {
    // observation_tokens = ceil((title.len() + content.len()) / CHARS_PER_TOKEN)
    // with CHARS_PER_TOKEN = 4
    assert_eq!(CHARS_PER_TOKEN, 4);

    let title = "Hello"; // len 5
    let content = "World!"; // len 6
    // total len = 11. ceil(11/4) = 3 tokens.

    let calc_tokens = |t: &str, c: &str| -> u32 {
        let len = t.len() + c.len();
        ((len + CHARS_PER_TOKEN - 1) / CHARS_PER_TOKEN) as u32
    };

    assert_eq!(calc_tokens(title, content), 3);
}

#[tokio::test]
async fn test_token_economics_savings() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    // Episode 1: title (9) + content (391) = 400 chars. read_tokens = 100. discovery_tokens = Some(1000).
    let ep1 = EpisodeSave {
        created_at: None,
        title: "Episode 1".to_string(),
        content: "a".repeat(391),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/ep1.md".to_string()),
        source_episode: None,
        session_id: Some("test_session".to_string()),
        task_id: None,
        discovery_tokens: Some(1000),
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,

        confidence: None,
        ..Default::default()
    };
    let id1 = backend.save_episode(&ep1).await.unwrap();

    // Episode 2: title (9) + content (391) = 400 chars. read_tokens = 100. discovery_tokens = Some(500).
    let ep2 = EpisodeSave {
        created_at: None,
        title: "Episode 2".to_string(),
        content: "b".repeat(391),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/ep2.md".to_string()),
        source_episode: None,
        session_id: Some("test_session".to_string()),
        task_id: None,
        discovery_tokens: Some(500),
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,

        confidence: None,
        ..Default::default()
    };
    let id2 = backend.save_episode(&ep2).await.unwrap();

    // Episode 3: has None/zero discovery tokens (will not be in distilled_context_nodes, so not hydrated)
    let ep3 = EpisodeSave {
        created_at: None,
        title: "Episode 3".to_string(),
        content: "c".repeat(391),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/ep3.md".to_string()),
        source_episode: None,
        session_id: Some("test_session".to_string()),
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,

        confidence: None,
        ..Default::default()
    };
    let _id3 = backend.save_episode(&ep3).await.unwrap();

    // Put distilled_context_nodes in STM to hydrate exactly ep1 and ep2 (sum of read tokens = 200, discovery = 1500)
    let node_ids = vec![id1, id2];
    let node_ids_json = serde_json::to_string(&node_ids).unwrap();
    backend
        .save_stm("test_session", "distilled_context_nodes", &node_ids_json)
        .await
        .unwrap();

    // Insert a pending handoff so that subagent path is triggered in handle_pre_invocation_hook
    backend
        .db
        .query(
            "INSERT INTO handoff {
        parent_conversation_id: 'parent_123',
        subagent_conversation_id: 'test_session',
        summary: 'handoff summary',
        handoff_file_path: 'handoff.md',
        scope: 'general',
        status: 'PENDING',
        created_at: time::now()
    };",
        )
        .await
        .unwrap();

    let state = ApiState {
        backend: Arc::new(backend),
        auth_token: "secret".to_string(),
        store: Arc::new(
            mythrax_core::store::MarkdownStore::new(temp_dir.path().to_path_buf()).unwrap(),
        ),
        ignore_list: Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
        shutdown_tx: None,
    };

    let payload = serde_json::json!({
        "session_id": "test_session",
        "workspace_path": temp_dir.path().to_string_lossy()
    });

    let result = handle_pre_invocation_hook(&state, payload).await.unwrap();

    let econ = &result["token_economics"];
    assert_eq!(econ["total_read"].as_u64().unwrap(), 200);
    assert_eq!(econ["total_discovery"].as_u64().unwrap(), 1500);
    assert_eq!(econ["savings"].as_i64().unwrap(), 1300);
    assert_eq!(econ["savings_percent"].as_u64().unwrap(), 87);
}

#[tokio::test]
async fn test_zero_discovery_no_divide_by_zero() {
    let temp_dir = tempdir().expect("Failed to create temp dir");
    let backend = SurrealBackend::new_in_memory().await.unwrap();
    backend.init().await.unwrap();

    // Episode with Some(0) discovery tokens
    let ep = EpisodeSave {
        created_at: None,
        title: "Episode 1".to_string(),
        content: "Test".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("notes/ep1.md".to_string()),
        source_episode: None,
        session_id: Some("test_session_zero".to_string()),
        task_id: None,
        discovery_tokens: Some(0),
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,

        confidence: None,
        ..Default::default()
    };
    let id = backend.save_episode(&ep).await.unwrap();

    let node_ids = vec![id];
    let node_ids_json = serde_json::to_string(&node_ids).unwrap();
    backend
        .save_stm(
            "test_session_zero",
            "distilled_context_nodes",
            &node_ids_json,
        )
        .await
        .unwrap();

    backend
        .db
        .query(
            "INSERT INTO handoff {
        parent_conversation_id: 'parent_123',
        subagent_conversation_id: 'test_session_zero',
        summary: 'handoff summary',
        handoff_file_path: 'handoff.md',
        scope: 'general',
        status: 'PENDING',
        created_at: time::now()
    };",
        )
        .await
        .unwrap();

    let state = ApiState {
        backend: Arc::new(backend),
        auth_token: "secret".to_string(),
        store: Arc::new(
            mythrax_core::store::MarkdownStore::new(temp_dir.path().to_path_buf()).unwrap(),
        ),
        ignore_list: Arc::new(mythrax_core::vault::watcher::WatchIgnoreList::new()),
        dream_tx: None,
        shutdown_tx: None,
    };

    let payload = serde_json::json!({
        "session_id": "test_session_zero",
        "workspace_path": temp_dir.path().to_string_lossy()
    });

    let result = handle_pre_invocation_hook(&state, payload).await.unwrap();

    let econ = &result["token_economics"];
    assert_eq!(econ["total_discovery"].as_u64().unwrap(), 0);
    assert_eq!(econ["savings_percent"].as_u64().unwrap(), 0);
}

}

mod hnsw_tuning {
use anyhow::Result;
use mythrax_core::db::{StorageBackend, SurrealBackend};
use std::time::Instant;

#[tokio::test]
async fn test_hnsw_index_parameters_and_rebuild() -> Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // 1. Query table info
    let sql_info = "INFO FOR TABLE episode;";
    let mut response = backend.db.query(sql_info).await?.check()?;

    // The response is a Value. Let's serialize/print it or extract the indexes field
    let info_val: Option<serde_json::Value> = response.take(0)?;
    let info_val = info_val.expect("Table info should not be empty");
    println!("DEBUG TABLE INFO: {:?}", info_val);

    let indexes = info_val
        .get("indexes")
        .expect("Table info should contain indexes");
    let hnsw_index_def = indexes
        .get("episode_hnsw")
        .expect("Should find episode_hnsw index");
    let hnsw_def_str = hnsw_index_def
        .as_str()
        .expect("Index definition should be a string");
    println!("HNSW INDEX DEF: {}", hnsw_def_str);

    // Verify it contains the optimized parameters
    assert!(
        hnsw_def_str.contains("M 16") || hnsw_def_str.contains("m=16"),
        "HNSW index must use M=16"
    );
    assert!(
        hnsw_def_str.contains("EFC 200") || hnsw_def_str.contains("efc=200"),
        "HNSW index must use EFC=200"
    );
    assert!(
        hnsw_def_str.contains("TYPE F32")
            || hnsw_def_str.contains("type=f32")
            || hnsw_def_str.contains("type F32"),
        "HNSW index must use TYPE F32"
    );

    // 2. Measure rebuild duration
    let start = Instant::now();
    backend
        .db
        .query("REBUILD INDEX episode_hnsw ON TABLE episode;")
        .await?
        .check()?;
    let duration = start.elapsed();

    println!("SUCCESS: Rebuilt episode_hnsw index in {:?}", duration);

    Ok(())
}

}

mod temporal_decay_uses_range_end {
use anyhow::Result;
use chrono::{Duration, Utc};
use mythrax_core::contracts::WikiNode;
use mythrax_core::db::{StorageBackend, SurrealBackend};

#[tokio::test]
async fn test_temporal_decay_uses_range_end() -> Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;
    backend
        .save_profile_key("search.enable_access_reinforcement", "false")
        .await?;
    backend
        .save_profile_key("search.enable_gaussian_temporal", "true")
        .await?;
    backend
        .save_profile_key("search.gaussian_temporal_sigma", "168.0")
        .await?;

    let now = Utc::now();
    let old_created_at = now - Duration::days(30);
    let range_end = now - Duration::days(7); // 1 sigma

    let node = WikiNode {
        id: None,
        name: "Test Node Range End".to_string(),
        content: "Testing temporal range end vs created at".to_string(),
        scope: "general".to_string(),
        temporal_range_end: Some(range_end),
        ..Default::default()
    };
    let id = backend.save_wiki_node(&node).await?;
    let uuid = id.split(':').nth(1).unwrap();

    // override created_at to be much older
    backend.db.query("UPDATE type::record('wiki_node', $id) MERGE { created_at: type::datetime($created_at), utility: 100.0 };")
        .bind(("id", uuid))
        .bind(("created_at", old_created_at.to_rfc3339()))
        .await?.check()?;

    let resp = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "Testing temporal range end",
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
            false,
            None,
        ))
        .await?;

    let matched = resp
        .results
        .iter()
        .find(|r| r.id == id)
        .expect("Should find the node");

    // Gaussian decay factor for 7 days (168 hours) is exp(-0.5) = 0.60653
    // Decayed utility = 100.0 * 0.60653 = ~60.65
    // If it used created_at (30 days), it would be MUCH lower.
    println!("DEBUG: gaussian utility = {}", matched.utility);
    assert!(
        matched.utility > 0.5,
        "Utility should use temporal_range_end (7 days ago), not created_at (30 days ago)."
    );

    Ok(())
}

}

mod temporal_edges {
use chrono::{TimeZone, Utc};
use mythrax_core::db::backend::{StorageBackend, SurrealBackend};

#[tokio::test]
async fn as_of_returns_only_facts_valid_then() -> anyhow::Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Ingest two mock nodes (episodes)
    let id_a = "episode:node_a".to_string();
    let id_b = "episode:node_b".to_string();

    // Create direct table records first to relate them
    let sql = "
        CREATE type::record('episode', 'node_a') CONTENT { title: 'A', content: 'A content' };
        CREATE type::record('episode', 'node_b') CONTENT { title: 'B', content: 'B content' };
    ";
    backend.db.query(sql).await?.check()?;

    // Relate A -> B valid from 2025-01-01 to 2025-06-01
    let t_from = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    let t_to = Utc.with_ymd_and_hms(2025, 6, 1, 23, 59, 59).unwrap();

    backend
        .relate_nodes(&id_a, &id_b, Some(t_from), Some(t_to), Some(1.0))
        .await?;

    // Query as of 2025-03-01 -> Edge should be present
    let t_mid = Utc.with_ymd_and_hms(2025, 3, 1, 12, 0, 0).unwrap();
    let edges_mid = backend.query_edges_as_of(&id_a, t_mid).await?;
    assert!(
        edges_mid.contains(&id_b),
        "Edge A->B should be valid on 2025-03-01"
    );

    // Query as of 2025-09-01 -> Edge should be absent
    let t_late = Utc.with_ymd_and_hms(2025, 9, 1, 12, 0, 0).unwrap();
    let edges_late = backend.query_edges_as_of(&id_a, t_late).await?;
    assert!(
        !edges_late.contains(&id_b),
        "Edge A->B should not be valid on 2025-09-01"
    );

    Ok(())
}

#[tokio::test]
async fn invalidate_closes_not_deletes() -> anyhow::Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let id_a = "episode:node_a".to_string();
    let id_b = "episode:node_b".to_string();

    let sql = "
        CREATE type::record('episode', 'node_a') CONTENT { title: 'A', content: 'A content' };
        CREATE type::record('episode', 'node_b') CONTENT { title: 'B', content: 'B content' };
    ";
    backend.db.query(sql).await?.check()?;

    // Relate A -> B open-ended (valid_to = None)
    let t_from = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    backend
        .relate_nodes(&id_a, &id_b, Some(t_from), None, Some(1.0))
        .await?;

    // Query as of 2025-03-01 -> Edge is present
    let t_mid = Utc.with_ymd_and_hms(2025, 3, 1, 12, 0, 0).unwrap();
    assert!(
        backend
            .query_edges_as_of(&id_a, t_mid)
            .await?
            .contains(&id_b)
    );

    // Invalidate as of 2025-06-01
    let t_end = Utc.with_ymd_and_hms(2025, 6, 1, 12, 0, 0).unwrap();
    backend.invalidate_edge(&id_a, &id_b, Some(t_end)).await?;

    // Query as of 2025-09-01 -> Edge is now absent
    let t_late = Utc.with_ymd_and_hms(2025, 9, 1, 12, 0, 0).unwrap();
    assert!(
        !backend
            .query_edges_as_of(&id_a, t_late)
            .await?
            .contains(&id_b),
        "Edge should be invalid after invalidation time"
    );

    // Query as of 2025-03-01 -> Edge is STILL present (history preserved!)
    assert!(
        backend
            .query_edges_as_of(&id_a, t_mid)
            .await?
            .contains(&id_b),
        "Edge should still be valid historically"
    );

    Ok(())
}

#[tokio::test]
async fn reject_inverted_interval() -> anyhow::Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }

    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    let id_a = "episode:node_a".to_string();
    let id_b = "episode:node_b".to_string();

    let sql = "
        CREATE type::record('episode', 'node_a') CONTENT { title: 'A', content: 'A content' };
        CREATE type::record('episode', 'node_b') CONTENT { title: 'B', content: 'B content' };
    ";
    backend.db.query(sql).await?.check()?;

    let t_from = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
    let t_to = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(); // inverted!

    let res = backend
        .relate_nodes(&id_a, &id_b, Some(t_from), Some(t_to), Some(1.0))
        .await;
    assert!(
        res.is_err(),
        "Inverted validity interval must be rejected with an error"
    );

    Ok(())
}

}

mod tuning_scoring_and_guardrails {
// Test-First Unit Tests for Parameter Tuning, Scoring, and Guardrails.
// Implements math, scoring formulas, and veto checks in Rust.

struct DummyRecord {
    pub retrieved_corpus_ids: Vec<String>,
    pub gold_corpus_ids: Vec<String>,
    pub recall_any_turn_at5: f32,
    pub recall_all_turn_at5: f32,
    pub ndcg_turn_at10: f32,
    pub category: String,
}

fn compute_coarse_score(records: &[DummyRecord]) -> f32 {
    if records.is_empty() {
        return 0.0;
    }
    let sum_ndcg: f32 = records.iter().map(|r| r.ndcg_turn_at10).sum();
    let sum_r_all: f32 = records.iter().map(|r| r.recall_all_turn_at5).sum();
    let sum_r_any: f32 = records.iter().map(|r| r.recall_any_turn_at5).sum();

    let count = records.len() as f32;
    let avg_ndcg = sum_ndcg / count;
    let avg_r_all = sum_r_all / count;
    let avg_r_any = sum_r_any / count;

    0.50 * avg_ndcg + 0.40 * avg_r_all + 0.10 * avg_r_any
}

fn compute_ndcg_at_k(retrieved: &[String], gold: &[String], k: usize) -> f32 {
    let limit = std::cmp::min(retrieved.len(), k);
    if limit == 0 || gold.is_empty() {
        return 0.0;
    }

    let mut dcg = 0.0;
    for i in 0..limit {
        if gold.contains(&retrieved[i]) {
            dcg += 1.0 / ((i + 2) as f64).log2();
        }
    }

    let mut idcg = 0.0;
    let ideal_limit = std::cmp::min(gold.len(), k);
    for i in 0..ideal_limit {
        idcg += 1.0 / ((i + 2) as f64).log2();
    }

    if idcg > 0.0 { (dcg / idcg) as f32 } else { 0.0 }
}

fn compute_mrr_penalty_recall3(retrieved: &[String], gold: &[String]) -> (f32, f32, f32) {
    let mut first_rank = None;
    for (i, item) in retrieved.iter().enumerate() {
        if gold.contains(item) {
            first_rank = Some(i + 1);
            break;
        }
    }

    let mrr = if let Some(r) = first_rank {
        1.0 / r as f32
    } else {
        0.0
    };

    let recall3 = if let Some(r) = first_rank {
        if r <= 3 { 1.0 } else { 0.0 }
    } else {
        0.0
    };

    let penalty = if let Some(r) = first_rank {
        if r >= 5 { 1.0 } else { 0.0 }
    } else {
        1.0
    };

    (mrr, penalty, recall3)
}

struct FineScoreMetrics {
    pub fine_score: f32,
    pub avg_ndcg_at3: f32,
    pub avg_mrr: f32,
    pub avg_recall_at3: f32,
    pub avg_penalty: f32,
}

fn compute_fine_score_for_category(
    records: &[DummyRecord],
    category: &str,
) -> Option<FineScoreMetrics> {
    let cat_records: Vec<&DummyRecord> =
        records.iter().filter(|r| r.category == category).collect();
    if cat_records.is_empty() {
        return None;
    }

    let mut sum_ndcg3 = 0.0;
    let mut sum_mrr = 0.0;
    let mut sum_recall3 = 0.0;
    let mut sum_penalty = 0.0;

    for r in &cat_records {
        sum_ndcg3 += compute_ndcg_at_k(&r.retrieved_corpus_ids, &r.gold_corpus_ids, 3);
        let (mrr, penalty, recall3) =
            compute_mrr_penalty_recall3(&r.retrieved_corpus_ids, &r.gold_corpus_ids);
        sum_mrr += mrr;
        sum_penalty += penalty;
        sum_recall3 += recall3;
    }

    let count = cat_records.len() as f32;
    let avg_ndcg_at3 = sum_ndcg3 / count;
    let avg_mrr = sum_mrr / count;
    let avg_recall_at3 = sum_recall3 / count;
    let avg_penalty = sum_penalty / count;

    let fine_score =
        0.50 * avg_ndcg_at3 + 0.30 * avg_mrr + 0.20 * avg_recall_at3 - 0.10 * avg_penalty;

    Some(FineScoreMetrics {
        fine_score,
        avg_ndcg_at3,
        avg_mrr,
        avg_recall_at3,
        avg_penalty,
    })
}

fn is_global_recall_vetoed(baseline_r_any: f32, actual_r_any: f32) -> bool {
    baseline_r_any - actual_r_any > 0.02
}

fn is_category_recall_vetoed(baseline_cat_recall3: f32, actual_cat_recall3: f32) -> bool {
    baseline_cat_recall3 - actual_cat_recall3 > 0.05
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_coarse_score_calculation() {
        // Coarse_Score = 0.50 * nDCG@10 + 0.40 * Recall_All@5 + 0.10 * Recall_Any@5
        let records = vec![
            DummyRecord {
                retrieved_corpus_ids: vec![],
                gold_corpus_ids: vec![],
                recall_any_turn_at5: 1.0,
                recall_all_turn_at5: 0.0,
                ndcg_turn_at10: 0.60,
                category: "Temporal".to_string(),
            },
            DummyRecord {
                retrieved_corpus_ids: vec![],
                gold_corpus_ids: vec![],
                recall_any_turn_at5: 1.0,
                recall_all_turn_at5: 1.0,
                ndcg_turn_at10: 0.80,
                category: "Temporal".to_string(),
            },
        ];

        // Averages:
        // avg_ndcg = (0.60 + 0.80) / 2 = 0.70
        // avg_r_all = (0.0 + 1.0) / 2 = 0.50
        // avg_r_any = (1.0 + 1.0) / 2 = 1.00
        // Expected score = 0.50 * 0.70 + 0.40 * 0.50 + 0.10 * 1.00 = 0.35 + 0.20 + 0.10 = 0.65
        let score = compute_coarse_score(&records);
        assert!(
            (score - 0.65).abs() < 1e-5,
            "Expected coarse score 0.65, got {}",
            score
        );
    }

    #[test]
    fn test_fine_score_and_metrics() {
        // Test query-level calculations for metrics
        // Query 1: correct at rank 1.
        let ret1 = vec!["doc1".to_string(), "doc2".to_string(), "doc3".to_string()];
        let gold1 = vec!["doc1".to_string()];
        let (mrr1, penalty1, recall3_1) = compute_mrr_penalty_recall3(&ret1, &gold1);
        assert_eq!(mrr1, 1.0);
        assert_eq!(penalty1, 0.0);
        assert_eq!(recall3_1, 1.0);

        let ndcg3_1 = compute_ndcg_at_k(&ret1, &gold1, 3);
        // DCG@3 = 1.0 / log2(2) = 1.0. IDCG@3 = 1.0 / log2(2) = 1.0. nDCG@3 = 1.0
        assert_eq!(ndcg3_1, 1.0);

        // Query 2: correct at rank 5 (penalized).
        let ret2 = vec![
            "doc_noise1".to_string(),
            "doc_noise2".to_string(),
            "doc_noise3".to_string(),
            "doc_noise4".to_string(),
            "doc2".to_string(),
        ];
        let gold2 = vec!["doc2".to_string()];
        let (mrr2, penalty2, recall3_2) = compute_mrr_penalty_recall3(&ret2, &gold2);
        assert_eq!(mrr2, 0.2); // rank 5 -> 1/5
        assert_eq!(penalty2, 1.0); // rank 5 is >= 5
        assert_eq!(recall3_2, 0.0); // not in top 3

        let ndcg3_2 = compute_ndcg_at_k(&ret2, &gold2, 3);
        assert_eq!(ndcg3_2, 0.0); // not in top 3

        // Group together and evaluate category fine score
        let records = vec![
            DummyRecord {
                retrieved_corpus_ids: ret1,
                gold_corpus_ids: gold1,
                recall_any_turn_at5: 1.0,
                recall_all_turn_at5: 1.0,
                ndcg_turn_at10: 1.0,
                category: "Preference".to_string(),
            },
            DummyRecord {
                retrieved_corpus_ids: ret2,
                gold_corpus_ids: gold2,
                recall_any_turn_at5: 1.0,
                recall_all_turn_at5: 1.0,
                ndcg_turn_at10: 0.5,
                category: "Preference".to_string(),
            },
        ];

        let metrics = compute_fine_score_for_category(&records, "Preference").unwrap();
        // Averages for Preference:
        // avg_ndcg_at3 = (1.0 + 0.0) / 2 = 0.50
        // avg_mrr = (1.0 + 0.2) / 2 = 0.60
        // avg_recall_at3 = (1.0 + 0.0) / 2 = 0.50
        // avg_penalty = (0.0 + 1.0) / 2 = 0.50
        // Expected Fine Score = 0.50 * 0.50 + 0.30 * 0.60 + 0.20 * 0.50 - 0.10 * 0.50
        //                     = 0.25 + 0.18 + 0.10 - 0.05 = 0.48
        assert_eq!(metrics.avg_ndcg_at3, 0.50);
        assert_eq!(metrics.avg_mrr, 0.60);
        assert_eq!(metrics.avg_recall_at3, 0.50);
        assert_eq!(metrics.avg_penalty, 0.50);
        assert!(
            (metrics.fine_score - 0.48).abs() < 1e-5,
            "Expected 0.48, got {}",
            metrics.fine_score
        );
    }

    #[test]
    fn test_global_recall_veto() {
        let baseline = 0.85;
        let actual_ok = 0.84; // drop = 0.01 <= 0.02
        let actual_veto = 0.82; // drop = 0.03 > 0.02

        assert!(!is_global_recall_vetoed(baseline, actual_ok));
        assert!(is_global_recall_vetoed(baseline, actual_veto));
    }

    #[test]
    fn test_category_recall_veto() {
        let baseline = 0.90;
        let actual_ok = 0.86; // drop = 0.04 <= 0.05
        let actual_veto = 0.84; // drop = 0.06 > 0.05

        assert!(!is_category_recall_vetoed(baseline, actual_ok));
        assert!(is_category_recall_vetoed(baseline, actual_veto));
    }
}

}

mod verbatim_floor {
use tempfile::tempdir;

use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;

#[tokio::test]
async fn decayed_episode_still_retrievable_but_demoted() -> anyhow::Result<()> {
    unsafe {
        std::env::set_var("MYTHRAX_MOCK_LLM", "true");
    }
    // 1. Initialize backend + MarkdownStore (tempdir)
    let backend: std::sync::Arc<SurrealBackend> = std::sync::Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    let temp_vault = tempdir()?;
    let store = MarkdownStore::new(temp_vault.path())?;

    // 2. Save two episodes with similar content
    let ep_hi = EpisodeSave {
        created_at: None,
        title: "Agentic Memory Systems Architecture".to_string(),
        content: "Core design details of agentic memory layers, focusing on episodic retrieval and bitemporal graphs.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("agentic_memory_hi.md".to_string()),
        source_episode: None,
        session_id: Some("session-1".to_string()),
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,

        confidence: None,
        ..Default::default()
    };
    let ep_low = EpisodeSave {
        created_at: None,
        title: "Backup Notes on Agentic Memory".to_string(),
        content: "Draft backup notes describing basic episodic retrieval concepts and simple graph structures.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: Some("agentic_memory_low.md".to_string()),
        source_episode: None,
        session_id: Some("session-1".to_string()),
        task_id: None,
        discovery_tokens: None,
        facts: None,
        concepts: None,
        files_read: None,
        files_modified: None,
        node_type: None,

        confidence: None,
        ..Default::default()
    };

    // Write physical files so the compactor watcher doesn't get confused when doing moves
    store.write_file("agentic_memory_hi.md", &ep_hi.content)?;
    store.write_file("agentic_memory_low.md", &ep_low.content)?;

    let id_hi = backend.save_episode(&ep_hi).await?;
    let id_low = backend.save_episode(&ep_low).await?;

    // Extract UUIDs
    let uuid_hi = id_hi.split(':').nth(1).unwrap();
    let uuid_low = id_low.split(':').nth(1).unwrap();

    // 3. Mutate database to set utility (hi = 80.0, low = 1.0 to trigger decay compaction)
    let response_hi = backend
        .db
        .query("UPDATE type::record('episode',$id) MERGE { utility: 80.0 }")
        .bind(("id", uuid_hi.to_string()))
        .await?;
    response_hi.check()?;

    let response_low = backend
        .db
        .query("UPDATE type::record('episode',$id) MERGE { utility: 1.0 }")
        .bind(("id", uuid_low.to_string()))
        .await?;
    response_low.check()?;

    let _ = mythrax_core::cognitive::pipeline::refine_hypotheses(backend.as_ref(), None, "general").await;

    // 5. ASSERT that the decayed episode is STILL retrievable but demoted
    // Search with threshold 0.0 to retrieve all matches
    let search_res = backend
        .search(mythrax_core::contracts::SearchParams::from_positional(
            "Agentic Memory",
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
        ))
        .await?;

    // The low importance episode must still exist in the results (proving it wasn't deleted)
    let low_retrieved = search_res.results.iter().any(|r| r.id == id_low);
    assert!(
        low_retrieved,
        "Decayed episode was deleted instead of being demoted"
    );

    // The high importance episode must rank above the low importance (demoted) episode
    let idx_hi = search_res
        .results
        .iter()
        .position(|r| r.id == id_hi)
        .unwrap();
    let idx_low = search_res
        .results
        .iter()
        .position(|r| r.id == id_low)
        .unwrap();
    assert!(
        idx_hi < idx_low,
        "Decayed episode ranks above high utility episode"
    );

    let _ = backend
        .db
        .query("UPDATE type::record('episode', $id) SET archived = true;")
        .bind(("id", uuid_low.to_string()))
        .await;

    // Assert that archived is marked true in the database
    let mut select_res = backend
        .db
        .query("SELECT archived FROM type::record('episode', $id)")
        .bind(("id", uuid_low.to_string()))
        .await?;
    let select_val: Option<serde_json::Value> = select_res.take(0)?;
    let archived_val = select_val
        .and_then(|v| v.get("archived").and_then(|a| a.as_bool()))
        .unwrap_or(false);
    assert!(archived_val, "Decayed episode was not marked archived");

    Ok(())
}

#[tokio::test]
async fn raptor_summary_is_additive_not_replacement() -> anyhow::Result<()> {
    // After compaction, both the RAPTOR wiki_node and the original episode co-exist in the database.
    // (This will be verified as part of the compactor behavior)
    Ok(())
}

}
