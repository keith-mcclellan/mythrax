use std::sync::Arc;
use tempfile::tempdir;
use mythrax_core::db::backend::{SurrealBackend, StorageBackend};
use mythrax_core::store::MarkdownStore;
use mythrax_core::vault::watcher::WatchIgnoreList;
use mythrax_core::bench::agent_recall::{run_agent_recall, RecallQuery};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("=== ARBOR LOG CAP TUNING SWEEP ===");

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
        "sess_recall_tune",
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

    // 5. Sweep over search limit (0, 1, 2, 5)
    println!("\nSweeping search limit (FTS count constraint)...");
    for limit in [0, 1, 2, 5] {
        let report = run_agent_recall(&backend, &raw_queries, false, 0.0, limit).await?;
        println!("  - Search Limit {}: Score = {:.1}% ({}/{})", limit, report.overall_score, report.total_passed, report.total_queries);
    }

    // 6. Sweep over search threshold parameter (0.0, 0.5, 0.8, 1.1)
    println!("\nSweeping search threshold...");
    for threshold in [0.0, 0.5, 1.0, 1.5] {
        let report = run_agent_recall(&backend, &raw_queries, false, threshold, 5).await?;
        println!("  - Threshold {:.1}: Score = {:.1}% ({}/{})", threshold, report.overall_score, report.total_passed, report.total_queries);
    }

    println!("\nTuning sweep complete!");
    Ok(())
}
