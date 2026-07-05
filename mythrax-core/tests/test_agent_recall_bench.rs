use std::sync::Arc;
use tempfile::tempdir;
use mythrax_core::db::backend::{StorageBackend, SurrealBackend};
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::WatchIgnoreList;
use mythrax_core::bench::agent_recall::{run_agent_recall, RecallQuery};

#[tokio::test]
async fn test_run_agent_recall_benchmark() -> anyhow::Result<()> {
    // 1. Initialize backend
    let backend = Arc::new(SurrealBackend::new_in_memory().await?);
    backend.init().await?;
    backend.set_search_mode("hybrid").await;

    // 2. Setup MarkdownStore and WatchIgnoreList
    let vault_dir = tempdir()?;
    let store = Arc::new(MarkdownStore::new(vault_dir.path())?);
    let ignore = WatchIgnoreList::new();

    // 3. Mine the synthetic transcript
    let transcript_path = "bench_data/agent_recall_transcript.jsonl";
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    let transcript_full_path = std::path::PathBuf::from(&manifest_dir).join(transcript_path);
    
    let count = mythrax_core::hooks::precompact::mine_transcript(
        "sess_recall_test",
        &transcript_full_path.to_string_lossy(),
        backend.as_ref(),
        &store,
        &ignore,
    ).await?;

    println!("Successfully mined {} episodes from transcript.", count);

    // Allow FTS indexing
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    // 4. Load queries
    let queries_path = std::path::PathBuf::from(&manifest_dir).join("bench_data/agent_recall_queries.json");
    let queries_data = std::fs::read_to_string(queries_path)?;
    let raw_queries: Vec<RecallQuery> = serde_json::from_str(&queries_data)?;

    // 5. Run standard benchmark
    println!("\n=== RUNNING AGENT RECALL MICROBENCHMARK ===");
    let report = run_agent_recall(&backend, &raw_queries, false, 0.0, 5).await?;
    println!("\n=== SUMMARY ===");
    for (q_type, &(passed, total, pct)) in &report.scores_by_type {
        println!("  - {}: {} / {} ({:.1}%)", q_type, passed, total, pct);
    }
    println!("  - OVERALL SCORE: {} / {} ({:.1}%)", report.total_passed, report.total_queries, report.overall_score);
    println!("=====================================");

    assert!(report.total_queries > 0);

    // 6. Run automated sweep loop if MYTHRAX_RUN_SWEEP=1 is configured
    let run_sweep = std::env::var("MYTHRAX_RUN_SWEEP").map(|v| v == "1" || v == "true").unwrap_or(false);
    if run_sweep {
        println!("\n=== RUNNING SWEEP OVER TRAVERSAL DEPTH (1 to 4) ===");
        for depth in 1..=4 {
            // Write search.traversal_depth setting into profile table
            let sql = "UPSERT type::record('profile', 'search.traversal_depth') CONTENT { key: 'search.traversal_depth', value: $val };";
            backend.db.query(sql).bind(("val", depth.to_string())).await?.check()?;

            let report_sweep = run_agent_recall(&backend, &raw_queries, true, 0.0, 5).await?;
            println!("  - Traversal Depth {}: overall score = {:.1}% ({} / {})", depth, report_sweep.overall_score, report_sweep.total_passed, report_sweep.total_queries);
        }
    }

    Ok(())
}
