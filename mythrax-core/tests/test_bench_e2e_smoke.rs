use mythrax_core::bench::metrics::evaluate_retrieval;
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::backend::{StorageBackend, SurrealBackend};

// CB-1: exercise the EXACT code path the bench runner uses — ingest with
// `vault_path = corpus_id`, run the runner's `search(...)` call, and map results
// back to corpus ids via `vault_path` (NOT `r.id`). This validates the real
// vault_path -> corpus mapping the runner relies on, and asserts a concrete score.
#[tokio::test]
async fn test_bench_e2e_smoke_vault_path_mapping() -> anyhow::Result<()> {
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // Ingest 3 episodes, each stamped with a distinct corpus_id in vault_path,
    // exactly as runner.rs does.
    let fixtures = [
        (
            "sess_a_turn_0",
            "Advanced Memory Design",
            "agentic memory layers, episodic retrieval, bitemporal graphs, and compaction.",
        ),
        (
            "sess_b_turn_0",
            "Okapi BM25 Lexical Scoring",
            "Okapi BM25 ranks documents by relevance to a search query.",
        ),
        (
            "sess_c_turn_0",
            "Bitemporal Knowledge Graphs",
            "as-of queries over a complete audit trail of when data was recorded.",
        ),
    ];
    for (corpus_id, title, content) in &fixtures {
        let ep = EpisodeSave {
            title: title.to_string(),
            content: content.to_string(),
            scope: Some("general".to_string()),
            vault_path: Some(corpus_id.to_string()),
            session_id: Some("session-123".to_string()),
            ..Default::default()
        };
        backend.save_episode(&ep).await?;
    }

    // The runner's exact search call signature.
    let response = backend
        .search(
            "advanced memory bitemporal",
            Some("general"),
            false, // deep_insight
            10,    // limit (over-fetch to max(k_recall, k_ndcg))
            0,     // offset
            0.0,   // threshold
            None,  // token_budget
            false, // allow_downward
            true,  // include_episodes
            true,  // include_artifacts
        )
        .await?;

    assert!(response.total_matches > 0);
    assert!(!response.results.is_empty());

    // Map via vault_path, exactly like the runner (BI mapping path under test).
    let retrieved_corpus_ids: Vec<String> = response
        .results
        .iter()
        .filter_map(|r| r.vault_path.clone())
        .collect();
    assert!(
        !retrieved_corpus_ids.is_empty(),
        "search must return vault_path-mapped corpus ids (the runner depends on this)"
    );
    // Every returned id must be one of the ingested corpus ids (no silent zeroing/misalignment).
    for id in &retrieved_corpus_ids {
        assert!(
            fixtures.iter().any(|(cid, _, _)| cid == id),
            "unexpected corpus id {} not among ingested fixtures",
            id
        );
    }

    let rankings: Vec<usize> = (0..retrieved_corpus_ids.len()).collect();
    let gold = vec!["sess_a_turn_0".to_string(), "sess_c_turn_0".to_string()];
    let score = evaluate_retrieval(&rankings, &gold, &retrieved_corpus_ids, 5);

    // Both relevant docs are in a 3-doc corpus retrieved at k=5, so recall must be exact 1.0.
    assert!(score.recall_any.is_finite() && score.recall_all.is_finite() && score.ndcg.is_finite());
    assert_eq!(score.recall_any, 1.0);
    assert_eq!(score.recall_all, 1.0);
    assert!(score.ndcg > 0.0);

    Ok(())
}
