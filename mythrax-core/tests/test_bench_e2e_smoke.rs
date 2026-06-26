use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::bench::metrics::evaluate_retrieval;

#[tokio::test]
async fn test_bench_e2e_smoke() -> anyhow::Result<()> {
    // 1. Initialize in-memory backend
    let backend = SurrealBackend::new_in_memory().await?;
    backend.init().await?;

    // 2. Ingest 3 episodes
    let ep1 = EpisodeSave {
        title: "Introduction to Advanced Memory".to_string(),
        content: "This document describes the design of agentic memory layers, focusing on episodic retrieval, bitemporal graphs, and compaction.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: None,
        source_episode: None,
        session_id: Some("session-123".to_string()),
        task_id: Some("task-456".to_string()),
        ..Default::default()
    };
    let ep2 = EpisodeSave {
        title: "Okapi BM25 Lexical Scoring".to_string(),
        content: "Okapi BM25 is a ranking function used by search engines to estimate the relevance of documents to a given search query.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: None,
        source_episode: None,
        session_id: Some("session-123".to_string()),
        task_id: Some("task-456".to_string()),
        ..Default::default()
    };
    let ep3 = EpisodeSave {
        title: "Bitemporal Knowledge Graphs".to_string(),
        content: "Bitemporal modeling allows querying data at a specific point in time (as-of) while maintaining a complete audit trail of when data was recorded.".to_string(),
        entities: vec![],
        scope: Some("general".to_string()),
        vault_path: None,
        source_episode: None,
        session_id: Some("session-123".to_string()),
        task_id: Some("task-456".to_string()),
        ..Default::default()
    };

    let id1 = backend.save_episode(&ep1).await?;
    let _id2 = backend.save_episode(&ep2).await?;
    let id3 = backend.save_episode(&ep3).await?;

    // 3. Perform a search
    let response = backend.search(
        "advanced memory bitemporal",
        Some("general"),
        false, // deep_insight
        5,     // limit
        0,     // offset
        0.0,   // threshold (allow all matches)
        None,  // token_budget
        false, // allow_downward
        true,  // include_episodes
        true,  // include_artifacts
    ).await?;

    assert!(response.total_matches > 0);
    assert!(!response.results.is_empty());

    // 4. Map results to corpus IDs to run evaluate_retrieval
    let corpus_ids: Vec<String> = response.results.iter().map(|r| r.id.clone()).collect();
    let rankings: Vec<usize> = (0..corpus_ids.len()).collect(); // rankings match search output order
    let gold = vec![id1, id3]; // ep1 and ep3 are highly relevant to the query

    let score = evaluate_retrieval(&rankings, &gold, &corpus_ids, 3);
    assert!(score.recall_any >= 0.0 && score.recall_any <= 1.0);
    assert!(score.recall_all >= 0.0 && score.recall_all <= 1.0);
    assert!(score.ndcg >= 0.0 && score.ndcg.nd2g_finite().is_finite()); // custom or helper check

    Ok(())
}

// Add a helper trait or method to assert finite float scores in test
trait ScoreExt {
    fn nd2g_finite(&self) -> f32;
}
impl ScoreExt for f32 {
    fn nd2g_finite(&self) -> f32 {
        *self
    }
}
