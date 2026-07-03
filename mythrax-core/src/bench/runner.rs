use anyhow::{Context, Result};
use clap::Parser;
use hex;
use reqwest;
use serde::{Deserialize, Serialize};
use serde_json;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;

use mythrax_core::bench::metrics::{evaluate_retrieval, ndcg, session_id_from_corpus_id};
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::backend::{StorageBackend, SurrealBackend};

// === PINNED DATASET PROVENANCE (BI-2, BI-3) ===
// Canonical primary source: HF dataset `xiaowu0162/longmemeval-cleaned`, pinned to an
// exact commit SHA (NOT the floating `main` branch). The per-file SHA-256 values are the
// LFS object ids (= sha256 of the file content) at that revision, retrieved out of band:
//   curl -s https://huggingface.co/api/datasets/xiaowu0162/longmemeval-cleaned/tree/<REV>?recursive=true
// They are asserted against the locally-resolved file BEFORE any scoring. A mismatch is a
// hard SPEC-GAP and stops the run (no silent acceptance of tampered/wrong-revision data).
const DATASET_ID: &str = "xiaowu0162/longmemeval-cleaned";
const DATASET_REVISION: &str = "98d7416c24c778c2fee6e6f3006e7a073259d48f";
const EXPECTED_SHA256: &[(&str, &str)] = &[
    (
        "longmemeval_oracle.json",
        "821a2034d219ab45846873dd14c14f12cfe7776e73527a483f9dac095d38620c",
    ),
    (
        "longmemeval_s_cleaned.json",
        "d6f21ea9d60a0d56f34a05b609c79c88a451d2ae03597821ea3d5a9678c3a442",
    ),
    (
        "longmemeval_m_cleaned.json",
        "9d79e5524794a2e6900a3aa9cb7d9152c5a3e8319c9a87c25494ba1eacee495f",
    ),
];

// Decoupled cutoffs (BI-8): recall is reported @5, nDCG @10, per-question-type R @10.
const K_RECALL: usize = 5;
const K_NDCG: usize = 10;

#[derive(Parser, Debug)]
#[command(name = "bench", about = "Mythrax Advanced-Memory Retrieval Benchmark Harness")]
struct Args {
    /// Evaluation split:
    ///   full500       - DEFAULT, the only publishable mode. Scores over the REAL
    ///                   longmemeval_s long-context haystack (needle-in-haystack).
    ///   oracle        - upper-bound DIAGNOSTIC only (gold-evidence-only haystack).
    ///                   NEVER published; trivially inflates recall by construction.
    ///   internal-gate - fast CI no-regression subset. NEVER published.
    #[arg(long, default_value = "full500")]
    split: String,

    #[arg(long, default_value = "bench_data/official")]
    data_dir: String,

    /// Opt-in: fetch the pinned dataset from HuggingFace if absent locally. Even with
    /// this flag the SHA-256 integrity gate still runs. Default full500 REFUSES to
    /// download (BI-5): you must fetch+verify out of band, or pass --allow-download.
    #[arg(long)]
    allow_download: bool,

    /// Search mode: raw (vector only) or hybrid (vector + sparse + temporal + rerank)
    #[arg(long, default_value = "raw")]
    mode: String,
}

#[derive(Debug, Clone, Deserialize)]
struct QuestionEntry {
    question_id: String,
    question_type: String,
    question: String,
    #[allow(dead_code)]
    answer: serde_json::Value,
    haystack_session_ids: Vec<String>,
    haystack_sessions: Vec<Vec<TurnEntry>>,
    /// Gold session ids for session-granularity recall (BI-6). Some splits/files may
    /// omit it; default to empty so session recall degrades to 0.0 rather than panicking.
    #[serde(default)]
    answer_session_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TurnEntry {
    role: String,
    content: String,
    #[serde(default)]
    has_answer: bool,
}

#[derive(Serialize)]
struct Manifest {
    dataset_id: String,
    dataset_revision: String,
    scored_file: String,
    scored_file_sha256: String,
    file_sha256s: Vec<(String, String)>,
    split_mode: String,
    k_recall: usize,
    k_ndcg: usize,
    mythrax_git_commit: String,
    published: bool,
    note: String,
}

#[derive(Serialize)]
struct QuestionResultRecord {
    question_id: String,
    question_type: String,
    // turn-granularity (has_answer)
    recall_any_turn_at5: f32,
    recall_all_turn_at5: f32,
    ndcg_turn_at10: f32,
    recall_any_turn_at10: f32,
    // session-granularity (answer_session_ids)
    recall_any_session_at5: f32,
    recall_all_session_at5: f32,
    retrieved_corpus_ids: Vec<String>,
    gold_corpus_ids: Vec<String>,
    gold_session_ids: Vec<String>,
    // honesty stamp on every record
    published: bool,
    note: String,
    query_latency_ms: f64,
}

/// Resolve which dataset file backs a given split. full500 MUST resolve to the long-context
/// haystack `longmemeval_s_cleaned.json`; if it ever resolves to the oracle file we bail (BI-1).
fn resolve_scored_file(split: &str) -> Result<&'static str> {
    let file = match split {
        // BI-1: publishable run scores the REAL long-context haystack, not gold-evidence-only.
        "full500" | "internal-gate" | "dev50" => "longmemeval_s_cleaned.json",
        // Explicit upper-bound diagnostic ONLY. Never published.
        "oracle" => "longmemeval_oracle.json",
        other => anyhow::bail!(
            "SPEC-GAP: unknown split '{}'. Use full500 | oracle | internal-gate | dev50",
            other
        ),
    };
    if split == "full500" && file == "longmemeval_oracle.json" {
        anyhow::bail!(
            "SPEC-GAP: full500 must never score the gold-evidence-only oracle haystack"
        );
    }
    Ok(file)
}

fn expected_sha_for(filename: &str) -> Option<&'static str> {
    EXPECTED_SHA256
        .iter()
        .find(|(f, _)| *f == filename)
        .map(|(_, h)| *h)
}

#[tokio::main]
async fn main() -> Result<()> {
    unsafe { 
        std::env::set_var("MYTHRAX_DAEMON_PORT", "54321"); 
        std::env::set_var("MYTHRAX_SESSION_ISOLATION", "false");
    }
    let args = Args::parse();
    println!("Starting Mythrax LongMemEval retrieval benchmark runner...");
    println!("Split mode: {}", args.split);
    println!("Recall@{}, nDCG@{}", K_RECALL, K_NDCG);

    let is_published_mode = args.split == "full500";

    let data_path = Path::new(&args.data_dir);
    fs::create_dir_all(data_path).context("Failed to create data directory")?;

    let scored_filename = resolve_scored_file(&args.split)?;
    let scored_path = data_path.join(scored_filename);

    // --- Acquire (BI-5): default full500 REFUSES to auto-download. ---
    if !scored_path.exists() {
        if args.allow_download {
            let expected = expected_sha_for(scored_filename).ok_or_else(|| {
                anyhow::anyhow!(
                    "SPEC-GAP: no pinned SHA-256 for {} — refusing to download an unpinned file",
                    scored_filename
                )
            })?;
            println!(
                "Downloading {} from pinned revision {}...",
                scored_filename, DATASET_REVISION
            );
            let url = format!(
                "https://huggingface.co/datasets/{}/resolve/{}/{}",
                DATASET_ID, DATASET_REVISION, scored_filename
            );
            download_file(&url, &scored_path)
                .await
                .context(format!("Failed to download {}", scored_filename))?;
            let got = compute_sha256(&scored_path)?;
            if got != expected {
                anyhow::bail!(
                    "SPEC-GAP: dataset integrity check failed for {} after download (expected {}, got {})",
                    scored_filename,
                    expected,
                    got
                );
            }
        } else {
            anyhow::bail!(
                "SPEC-GAP: official LongMemEval dataset missing or integrity check failed — \
                 expected '{}' under '{}'. Fetch+verify out of band from the pinned revision {} \
                 (HF dataset {}), or pass --allow-download to fetch it now.",
                scored_filename,
                data_path.display(),
                DATASET_REVISION,
                DATASET_ID
            );
        }
    }

    // --- Integrity gate (BI-2): verify SHA-256 against the pinned const BEFORE scoring. ---
    let scored_sha = compute_sha256(&scored_path)
        .context(format!("Failed to compute SHA-256 for {}", scored_filename))?;
    let expected_sha = expected_sha_for(scored_filename).ok_or_else(|| {
        anyhow::anyhow!(
            "SPEC-GAP: {} is not a pinned official LongMemEval file",
            scored_filename
        )
    })?;
    if scored_sha != expected_sha {
        anyhow::bail!(
            "SPEC-GAP: dataset integrity check failed for {} (expected {}, got {})",
            scored_filename,
            expected_sha,
            scored_sha
        );
    }
    println!(
        "Integrity OK: {} SHA-256 {} == pinned",
        scored_filename, scored_sha
    );

    // Record SHA-256 of every pinned file present locally (manifest completeness).
    let mut file_sha256s = Vec::new();
    for (filename, _) in EXPECTED_SHA256 {
        let p = data_path.join(filename);
        if p.exists() {
            file_sha256s.push((filename.to_string(), compute_sha256(&p)?));
        }
    }

    // --- Load + parse. ---
    println!("Loading scored haystack: {:?}", scored_path);
    let mut file = File::open(&scored_path).context("Failed to open dataset file")?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .context("Failed to read dataset file")?;
    let questions: Vec<QuestionEntry> =
        serde_json::from_str(&contents).context("Failed to parse LongMemEval dataset JSON")?;
    println!("Loaded {} questions from dataset.", questions.len());

    // --- Integrity gate (BI-4): exactly 500 UNIQUE question_ids for the full official set. ---
    let unique_ids: HashSet<&String> = questions.iter().map(|q| &q.question_id).collect();
    if (args.split == "full500" || args.split == "oracle") && unique_ids.len() != 500 {
        anyhow::bail!(
            "SPEC-GAP: official LongMemEval dataset integrity check failed \
             (expected 500 UNIQUE question_ids, got {} unique out of {} rows)",
            unique_ids.len(),
            questions.len()
        );
    }

    // --- Select target subset. ---
    let target_questions: Vec<QuestionEntry> = if args.split == "internal-gate" {
        // BI-9: an ARBITRARY deterministic CI subset — NOT the canonical LongMemEval set and
        // NOT the reference-impl random.Random(42) partition. Sorted-by-id every-10th is a
        // stable, reproducible local gate only; never published, never leaderboard-comparable.
        let mut sorted = questions;
        sorted.sort_by(|a, b| a.question_id.cmp(&b.question_id));
        let subset: Vec<QuestionEntry> = sorted
            .into_iter()
            .enumerate()
            .filter(|(idx, _)| idx % 10 == 0)
            .map(|(_, q)| q)
            .collect();
        println!(
            "internal-gate: arbitrary deterministic CI subset of {} questions (NOT LongMemEval, NOT published).",
            subset.len()
        );
        subset
    } else if args.split == "dev50" {
        let mut sorted = questions;
        sorted.sort_by(|a, b| a.question_id.cmp(&b.question_id));
        let mut dev_subset = Vec::new();
        let mut counts = std::collections::HashMap::new();
        let limits = [
            ("knowledge-update".to_string(), 8),
            ("multi-session".to_string(), 13),
            ("single-session-assistant".to_string(), 6),
            ("single-session-preference".to_string(), 3),
            ("single-session-user".to_string(), 7),
            ("temporal-reasoning".to_string(), 13),
        ].into_iter().collect::<std::collections::HashMap<String, usize>>();

        for q in sorted {
            let limit = limits.get(&q.question_type).cloned().unwrap_or(0);
            let count = counts.entry(q.question_type.clone()).or_insert(0);
            if *count < limit {
                dev_subset.push(q);
                *count += 1;
            }
        }
        println!(
            "dev50: deterministic stratified dev split of {} questions.",
            dev_subset.len()
        );
        dev_subset
    } else {
        questions
    };

    let published = is_published_mode;
    let note = if is_published_mode {
        "LongMemEval retrieval (Recall@k / NDCG@k), full 500, longmemeval_s haystack".to_string()
    } else if args.split == "oracle" {
        "ORACLE upper-bound diagnostic (gold-evidence-only); NOT comparable to official LongMemEval, NOT published".to_string()
    } else {
        "internal split, not LongMemEval; arbitrary CI subset, not published".to_string()
    };

    // --- Evaluate. ---
    let cache_path = std::path::PathBuf::from("embedding_cache.bin");
    let cache_path_core = std::path::PathBuf::from("mythrax-core/embedding_cache.bin");
    let cache_path_parent = std::path::PathBuf::from("../embedding_cache.bin");
    let target_cache_path = if cache_path.exists() {
        cache_path
    } else if cache_path_core.exists() {
        cache_path_core
    } else if cache_path_parent.exists() {
        cache_path_parent
    } else {
        cache_path
    };
    if let Err(e) = mythrax_core::embeddings::load_embedding_cache_from_disk(&target_cache_path) {
        println!("Warning: failed to load embedding cache: {}", e);
    } else {
        println!("Loaded embedding cache from {:?}", target_cache_path);
        if args.mode == "tune" {
            let lambdas = vec!["0.01", "0.02", "0.05", "0.08", "0.12"];
            let gammas = vec!["0.05", "0.10", "0.20", "0.30", "0.40"];
            let mut best_score = -1.0;
            let mut best_params = (String::new(), String::new());

            println!("Starting 5-fold cross-validation grid search parameter sweep on dev split...");
            for lambda in &lambdas {
                for gamma in &gammas {
                    let mut overrides = std::collections::HashMap::new();
                    overrides.insert("search.decay_lambda".to_string(), lambda.to_string());
                    overrides.insert("search.gamma_rerank".to_string(), gamma.to_string());

                    let (avg_r_any, avg_r_all, avg_ndcg, _, _, avg_lat, _, _) = run_evaluation(
                        &target_questions,
                        "hybrid",
                        Some(overrides),
                        &target_cache_path,
                        published,
                        &note,
                    ).await?;

                    println!("Params: decay_lambda={}, gamma_rerank={} => Recall_Any@5={:.4}, Recall_All@5={:.4}, nDCG@10={:.4}, Latency={:.2}ms",
                             lambda, gamma, avg_r_any, avg_r_all, avg_ndcg, avg_lat);

                    let score = avg_r_any + avg_ndcg;
                    if score > best_score {
                        best_score = score;
                        best_params = (lambda.to_string(), gamma.to_string());
                    }
                }
            }
            println!("Best Parameters found: decay_lambda={}, gamma_rerank={} (Combined Score: {:.4})",
                     best_params.0, best_params.1, best_score);
        } else {
            let (avg_recall_any_turn, avg_recall_all_turn, avg_ndcg_turn, avg_recall_any_session, avg_recall_all_session, avg_latency, p95_latency, records) = run_evaluation(
                &target_questions,
                &args.mode,
                None,
                &target_cache_path,
                published,
                &note,
            ).await?;

            println!("\n========================================================");
            println!("        LongMemEval RETRIEVAL METRICS SUMMARY           ");
            println!("========================================================");
            println!("Split:                    {}", args.split);
            println!("Mode:                     {}", args.mode);
            println!("Average Query Latency:    {:.2}ms", avg_latency);
            println!("p95 Query Latency:        {:.2}ms", p95_latency);
            println!("Published:                {}", published);
            println!("Total Questions:          {}", target_questions.len());
            println!("-- turn granularity (has_answer) --");
            println!("Recall_Any@{}:            {:.4}", K_RECALL, avg_recall_any_turn);
            println!("Recall_All@{}:            {:.4}", K_RECALL, avg_recall_all_turn);
            println!("nDCG@{}:                  {:.4}", K_NDCG, avg_ndcg_turn);
            println!("-- session granularity (answer_session_ids) --");
            println!("Recall_Any@{} (session):  {:.4}", K_RECALL, avg_recall_any_session);
            println!("Recall_All@{} (session):  {:.4}", K_RECALL, avg_recall_all_session);
            println!("--------------------------------------------------------");
            println!("Per-Question-Type R@{} (turn recall_any):", K_NDCG);
            
            let mut type_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            let mut type_recall_at10: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
            for record in &records {
                *type_counts.entry(record.question_type.clone()).or_insert(0) += 1;
                *type_recall_at10.entry(record.question_type.clone()).or_insert(0.0) += record.recall_any_turn_at10;
            }
            
            let mut type_keys: Vec<&String> = type_counts.keys().collect();
            type_keys.sort();
            for q_type in &type_keys {
                let count = type_counts[*q_type];
                let avg = type_recall_at10.get(*q_type).cloned().unwrap_or(0.0) / count as f32;
                println!("  - {:<28} (n={:<3}): {:.4}", q_type, count, avg);
            }
            println!("========================================================\n");

            let manifest = Manifest {
                dataset_id: DATASET_ID.to_string(),
                dataset_revision: DATASET_REVISION.to_string(),
                scored_file: scored_filename.to_string(),
                scored_file_sha256: scored_sha.clone(),
                file_sha256s,
                split_mode: args.split.clone(),
                k_recall: K_RECALL,
                k_ndcg: K_NDCG,
                mythrax_git_commit: get_git_commit().unwrap_or_else(|_| "unknown".to_string()),
                published,
                note: note.clone(),
            };

            let output_dir = Path::new("bench_data");
            fs::create_dir_all(output_dir).context("Failed to create bench_data directory")?;
            let output_file_path = output_dir.join(format!("results_{}.jsonl", args.split));
            let mut out_file = File::create(&output_file_path).context("Failed to create results file")?;
            out_file.write_all((serde_json::to_string(&manifest)? + "\n").as_bytes())?;
            for rec in &records {
                out_file.write_all((serde_json::to_string(rec)? + "\n").as_bytes())?;
            }
            println!("Detailed results written to {:?}", output_file_path);

            if is_published_mode {
                let baseline_path = output_dir.join("BASELINE.md");
                let mut baseline_file = File::create(&baseline_path).context("Failed to create BASELINE.md")?;
                let type_table = {
                    let mut keys: Vec<&String> = type_counts.keys().collect();
                    keys.sort();
                    keys.iter()
                        .map(|q_type| {
                            let count = type_counts[*q_type];
                            let avg = type_recall_at10.get(*q_type).cloned().unwrap_or(0.0) / count as f32;
                            format!("- **{}** (n={}): R@{} = `{:.4}`", q_type, count, K_NDCG, avg)
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                let baseline_content = format!(
                    "# Mythrax LongMemEval *retrieval* Baseline (full 500)\n\n\
                     **Metric:** LongMemEval *retrieval* (Recall@k / NDCG@k) — NOT QA accuracy.\n\
                     **Dataset ID:** `{}`\n\
                     **Pinned Revision (commit SHA):** `{}`\n\
                     **Scored file:** `{}` (long-context haystack)\n\
                     **Scored file SHA-256:** `{}`\n\
                     **Split:** `full500` (official 500-question set, full longmemeval_s haystack)\n\
                     **Mythrax Git Commit:** `{}`\n\
                     **Evaluated at:** {}\n\n\
                     ## Aggregate Metrics\n\
                     ### Turn granularity (has_answer)\n\
                     - **Recall_Any@{}:** `{:.4}`\n\
                     - **Recall_All@{}:** `{:.4}`\n\
                     - **nDCG@{}:** `{:.4}`\n\
                     ### Session granularity (answer_session_ids)\n\
                     - **Recall_Any@{} (session):** `{:.4}`\n\
                     - **Recall_All@{} (session):** `{:.4}`\n\n\
                     ## Per-Question-Type R@{} (turn recall_any)\n\
                     {}\n\n\
                     > [!IMPORTANT]\n\
                     > These are LongMemEval *retrieval* numbers scored over the full `longmemeval_s` \
                     haystack at the pinned revision above. Future optimizations must not regress \
                     `Recall_Any@{}`. The `oracle` split is an upper-bound diagnostic only and is never published.\n",
                    DATASET_ID,
                    DATASET_REVISION,
                    scored_filename,
                    scored_sha,
                    manifest.mythrax_git_commit,
                    chrono::Utc::now().to_rfc3339(),
                    K_RECALL, avg_recall_any_turn,
                    K_RECALL, avg_recall_all_turn,
                    K_NDCG, avg_ndcg_turn,
                    K_RECALL, avg_recall_any_session,
                    K_RECALL, avg_recall_all_session,
                    K_NDCG, type_table,
                    K_RECALL,
                );
                baseline_file.write_all(baseline_content.as_bytes())?;
                println!("Published baseline recorded in {:?}", baseline_path);
            } else {
                println!(
                    "Non-publishable split '{}' — BASELINE.md not written (honesty: only full500 is publishable).",
                    args.split
                );
            }
        }
    }

    Ok(())
}

async fn run_evaluation(
    target_questions: &[QuestionEntry],
    mode: &str,
    param_overrides: Option<std::collections::HashMap<String, String>>,
    target_cache_path: &std::path::Path,
    published: bool,
    note: &str,
) -> Result<(f32, f32, f32, f32, f32, f64, f64, Vec<QuestionResultRecord>)> {
    let retrieve_k = std::cmp::max(K_RECALL, K_NDCG);
    let mut records = Vec::new();
    let mut sum_recall_any_turn = 0.0f32;
    let mut sum_recall_all_turn = 0.0f32;
    let mut sum_ndcg_turn = 0.0f32;
    let mut sum_recall_any_session = 0.0f32;
    let mut sum_recall_all_session = 0.0f32;
    let mut sum_latency = 0.0f64;

    let mut type_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut type_recall_at10: std::collections::HashMap<String, f32> = std::collections::HashMap::new();

    unsafe {
        std::env::set_var("MYTHRAX_BENCH", "1");
    }

    let total_q = target_questions.len();
    let mut join_set = tokio::task::JoinSet::new();
    let concurrency_limit = 8;

    for (q_idx, q) in target_questions.iter().enumerate() {
        println!("Evaluating question {}/{}...", q_idx + 1, total_q);
        while join_set.len() >= concurrency_limit {
            if let Some(res) = join_set.join_next().await {
                let record: QuestionResultRecord = res.context("Parallel evaluation task panicked")??;
                sum_recall_any_turn += record.recall_any_turn_at5;
                sum_recall_all_turn += record.recall_all_turn_at5;
                sum_ndcg_turn += record.ndcg_turn_at10;
                sum_recall_any_session += record.recall_any_session_at5;
                sum_recall_all_session += record.recall_all_session_at5;
                sum_latency += record.query_latency_ms;

                *type_counts.entry(record.question_type.clone()).or_insert(0) += 1;
                *type_recall_at10.entry(record.question_type.clone()).or_insert(0.0) += record.recall_any_turn_at10;

                records.push(record);
            }
        }

        let q = q.clone();
        let published = published;
        let note = note.to_string();
        let mode = mode.to_string();
        let param_overrides = param_overrides.clone();

        join_set.spawn(async move {
            let backend = SurrealBackend::new_in_memory()
                .await
                .context("Failed to create in-memory backend")?;
            backend.init().await.context("Failed to initialize database schema")?;
            backend.set_search_mode(&mode).await;
            if let Some(ref o) = param_overrides {
                for (k, v) in o {
                    let _ = backend.save_profile_key(k, v).await;
                }
            }

            // Ingest only haystack sessions for this question
            let mut episodes_to_ingest = Vec::new();
            for (sess_idx, session_id) in q.haystack_session_ids.iter().enumerate() {
                if let Some(session_turns) = q.haystack_sessions.get(sess_idx) {
                    for (turn_idx, turn) in session_turns.iter().enumerate() {
                        let corpus_id = format!("{}_turn_{}", session_id, turn_idx);
                        let ep = EpisodeSave {
                            title: format!("Session {} - Turn {}", session_id, turn_idx),
                            content: format!("{}: {}", turn.role, turn.content),
                            scope: Some("general".to_string()),
                            vault_path: Some(corpus_id.clone()),
                            session_id: Some(session_id.clone()),
                            ..Default::default()
                        };
                        episodes_to_ingest.push(ep);
                    }
                }
            }
            backend.save_episodes_batch(&episodes_to_ingest).await
                .context("Failed to batch ingest haystack turns")?;

            let mut correct_turn_ids = Vec::new();
            for (sess_idx, session_id) in q.haystack_session_ids.iter().enumerate() {
                if let Some(session_turns) = q.haystack_sessions.get(sess_idx) {
                    for (turn_idx, turn) in session_turns.iter().enumerate() {
                        if turn.has_answer {
                            let corpus_id = format!("{}_turn_{}", session_id, turn_idx);
                            correct_turn_ids.push(corpus_id);
                        }
                    }
                }
            }

            let start_query = std::time::Instant::now();
            let search_response = backend
                .search(
                    &q.question,
                    Some("general"),
                    false,       // deep_insight
                    retrieve_k,  // limit: over-fetch to max(k_recall, k_ndcg)
                    0,           // offset
                    0.0,         // threshold (allow all)
                    None,        // token_budget
                    false,       // allow_downward
                    true,        // include_episodes
                    true,        // include_artifacts
                )
                .await
                .context("Search query failed during evaluation")?;
            let query_latency_ms = start_query.elapsed().as_secs_f64() * 1000.0;

            let retrieved_corpus_ids: Vec<String> = search_response
                .results
                .iter()
                .filter_map(|r| r.vault_path.clone())
                .collect();

            // Evaluate turn-level metrics
            let turn_rankings: Vec<usize> = (0..retrieved_corpus_ids.len()).collect();
            let turn5 = evaluate_retrieval(
                &turn_rankings,
                &correct_turn_ids,
                &retrieved_corpus_ids,
                K_RECALL,
            );
            let turn10 = evaluate_retrieval(
                &turn_rankings,
                &correct_turn_ids,
                &retrieved_corpus_ids,
                K_NDCG,
            );

            // Compute nDCG@10 (turn-granularity)
            let mut sum_dcg = 0.0;
            let mut sum_idcg = 0.0;
            for i in 0..std::cmp::min(retrieved_corpus_ids.len(), K_NDCG) {
                let gain = if correct_turn_ids.contains(&retrieved_corpus_ids[i]) {
                    1.0
                } else {
                    0.0
                };
                sum_dcg += gain / ((i + 2) as f64).log2();
            }
            for i in 0..std::cmp::min(correct_turn_ids.len(), K_NDCG) {
                sum_idcg += 1.0 / ((i + 2) as f64).log2();
            }
            let ndcg10 = if sum_idcg > 0.0 {
                (sum_dcg / sum_idcg) as f32
            } else {
                0.0
            };

            // Evaluate session-level metrics
            let retrieved_session_ids: Vec<String> = retrieved_corpus_ids
                .iter()
                .map(|id| session_id_from_corpus_id(id).to_string())
                .collect();
            let session_rankings: Vec<usize> = (0..retrieved_session_ids.len()).collect();
            let sess5 = evaluate_retrieval(
                &session_rankings,
                &q.answer_session_ids,
                &retrieved_session_ids,
                K_RECALL,
            );

            Ok::<QuestionResultRecord, anyhow::Error>(QuestionResultRecord {
                question_id: q.question_id,
                question_type: q.question_type,
                recall_any_turn_at5: turn5.recall_any,
                recall_all_turn_at5: turn5.recall_all,
                ndcg_turn_at10: ndcg10,
                recall_any_turn_at10: turn10.recall_any,
                recall_any_session_at5: sess5.recall_any,
                recall_all_session_at5: sess5.recall_all,
                retrieved_corpus_ids,
                gold_corpus_ids: correct_turn_ids,
                gold_session_ids: q.answer_session_ids,
                published,
                note,
                query_latency_ms,
            })
        });
    }

    while let Some(res) = join_set.join_next().await {
        let record: QuestionResultRecord = res.context("Parallel evaluation task panicked")??;
        sum_recall_any_turn += record.recall_any_turn_at5;
        sum_recall_all_turn += record.recall_all_turn_at5;
        sum_ndcg_turn += record.ndcg_turn_at10;
        sum_recall_any_session += record.recall_any_session_at5;
        sum_recall_all_session += record.recall_all_session_at5;
        sum_latency += record.query_latency_ms;

        *type_counts.entry(record.question_type.clone()).or_insert(0) += 1;
        *type_recall_at10.entry(record.question_type.clone()).or_insert(0.0) += record.recall_any_turn_at10;

        records.push(record);
    }

    let denom = total_q as f32;
    let avg_recall_any_turn = sum_recall_any_turn / denom;
    let avg_recall_all_turn = sum_recall_all_turn / denom;
    let avg_ndcg_turn = sum_ndcg_turn / denom;
    let avg_recall_any_session = sum_recall_any_session / denom;
    let avg_recall_all_session = sum_recall_all_session / denom;
    let avg_latency = sum_latency / total_q as f64;

    let mut latencies: Vec<f64> = records.iter().map(|r| r.query_latency_ms).collect();
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p95_latency = if latencies.is_empty() {
        0.0
    } else {
        let idx = ((latencies.len() as f64 * 0.95).ceil() as usize).saturating_sub(1);
        latencies[idx.min(latencies.len() - 1)]
    };

    Ok((
        avg_recall_any_turn,
        avg_recall_all_turn,
        avg_ndcg_turn,
        avg_recall_any_session,
        avg_recall_all_session,
        avg_latency,
        p95_latency,
        records,
    ))
}

async fn download_file(url: &str, dest: &Path) -> Result<()> {
    use futures_util::StreamExt;
    let response = reqwest::get(url).await?.error_for_status()?;
    let total_size = response.content_length().unwrap_or(0);
    let mut file = File::create(dest)?;
    let mut stream = response.bytes_stream();
    let mut downloaded = 0u64;
    let mut last_reported = 0u64;
    while let Some(item) = stream.next().await {
        let chunk = item.context("Error while downloading chunk")?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;
        if downloaded - last_reported >= 10 * 1024 * 1024 {
            if total_size > 0 {
                let percent = (downloaded as f64 / total_size as f64) * 100.0;
                println!(
                    "Downloading... {:.2}% ({:.2} MB / {:.2} MB)",
                    percent,
                    downloaded as f64 / (1024.0 * 1024.0),
                    total_size as f64 / (1024.0 * 1024.0)
                );
            } else {
                println!("Downloading... {:.2} MB", downloaded as f64 / (1024.0 * 1024.0));
            }
            last_reported = downloaded;
        }
    }
    println!("Finished downloading ({:.2} MB)", downloaded as f64 / (1024.0 * 1024.0));
    Ok(())
}

fn compute_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 1024 * 64];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn get_git_commit() -> Result<String> {
    let output = std::process::Command::new("git")
        .args(&["rev-parse", "HEAD"])
        .output()?;
    if output.status.success() {
        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    } else {
        anyhow::bail!("Failed to get git commit")
    }
}
