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
use surrealdb_types::SurrealValue;

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
#[command(
    name = "bench",
    about = "Mythrax Advanced-Memory Retrieval Benchmark Harness"
)]
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
    question: String,
    category: String,
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
        anyhow::bail!("SPEC-GAP: full500 must never score the gold-evidence-only oracle haystack");
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
        ]
        .into_iter()
        .collect::<std::collections::HashMap<String, usize>>();

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
            // Decoupled coordinate sweep
            let tune_questions = &target_questions;

            let format_metrics = |r_any: f32,
                                  r_all: f32,
                                  ndcg: f32,
                                  r_any_sess: f32,
                                  r_all_sess: f32,
                                  records: &[QuestionResultRecord]|
             -> String {
                let mut type_counts = std::collections::HashMap::new();
                let mut type_recall_at10 = std::collections::HashMap::new();
                for record in records {
                    *type_counts.entry(record.question_type.clone()).or_insert(0) += 1;
                    *type_recall_at10
                        .entry(record.question_type.clone())
                        .or_insert(0.0) += record.recall_any_turn_at10;
                }
                let get_per_type = |t_name: &str| -> f32 {
                    let count = *type_counts.get(t_name).unwrap_or(&0);
                    if count > 0 {
                        *type_recall_at10.get(t_name).unwrap_or(&0.0) / count as f32
                    } else {
                        0.0
                    }
                };
                format!(
                    "Turn R@5={:.4}, Turn nDCG={:.4}, Sess R_any={:.4}, Sess R_all={:.4}, \
                     R@10[assist={:.4}, update={:.4}, temp={:.4}, pref={:.4}, multi={:.4}, user={:.4}]",
                    r_any,
                    ndcg,
                    r_any_sess,
                    r_all_sess,
                    get_per_type("single-session-assistant"),
                    get_per_type("knowledge-update"),
                    get_per_type("temporal-reasoning"),
                    get_per_type("single-session-preference"),
                    get_per_type("multi-session"),
                    get_per_type("single-session-user")
                )
            };

            let mut locked_overrides = std::collections::HashMap::new();

            // Phase A: Core Fusion
            println!("--- Phase A: Core Fusion Sweep ---");
            let mut best_score_a = -1.0;
            let mut winner_a = (0.55f32, 0.60f32, 0.10f32);

            let sigmoid_centers = vec![0.45f32, 0.55f32];
            let fusion_sigmoid_centers = vec![0.50f32, 0.60f32];
            let gammas = vec![0.05f32, 0.10f32, 0.20f32];

            for sc in &sigmoid_centers {
                for fsc in &fusion_sigmoid_centers {
                    for gamma in &gammas {
                        let mut overrides = locked_overrides.clone();
                        overrides.insert("search.sigmoid_center".to_string(), sc.to_string());
                        overrides
                            .insert("search.fusion_sigmoid_center".to_string(), fsc.to_string());
                        overrides.insert("search.gamma_rerank".to_string(), gamma.to_string());

                        let (r_any, r_all, ndcg, r_any_sess, r_all_sess, lat, _, records) =
                            run_evaluation(
                                tune_questions,
                                "hybrid",
                                Some(overrides),
                                &target_cache_path,
                                published,
                                &note,
                            )
                            .await?;

                        let score = 0.50 * ndcg + 0.40 * r_all + 0.10 * r_any;
                        let detail =
                            format_metrics(r_any, r_all, ndcg, r_any_sess, r_all_sess, &records);
                        println!(
                            "A: sigmoid_center={}, fusion_sigmoid_center={}, gamma_rerank={} => score={:.4} ({} Lat={:.2}ms)",
                            sc, fsc, gamma, score, detail, lat
                        );

                        if score > best_score_a {
                            best_score_a = score;
                            winner_a = (*sc, *fsc, *gamma);
                        }
                    }
                }
            }

            println!(
                "Winner Phase A: sigmoid_center={}, fusion_sigmoid_center={}, gamma_rerank={} (Score: {:.4})",
                winner_a.0, winner_a.1, winner_a.2, best_score_a
            );

            locked_overrides.insert("search.sigmoid_center".to_string(), winner_a.0.to_string());
            locked_overrides.insert(
                "search.fusion_sigmoid_center".to_string(),
                winner_a.1.to_string(),
            );
            locked_overrides.insert("search.gamma_rerank".to_string(), winner_a.2.to_string());

            // Phase B: MMR & Reranking
            println!("--- Phase B: MMR & Reranking Sweep ---");
            let mut best_score_b = -1.0;
            let mut winner_b = (1.00f32, 50, 0.40f32, 0.30f32);

            let mmr_lambdas = vec![1.00f32];
            let rerank_pool_sizes = vec![15, 20];
            let w_person_names = vec![0.20f32, 0.40f32];
            let w_keyword_overlaps = vec![0.15f32, 0.30f32];

            for mmr in &mmr_lambdas {
                for pool in &rerank_pool_sizes {
                    for w_pn in &w_person_names {
                        for w_ko in &w_keyword_overlaps {
                            let mut overrides = locked_overrides.clone();
                            overrides.insert("search.mmr_lambda".to_string(), mmr.to_string());
                            overrides
                                .insert("search.rerank_pool_size".to_string(), pool.to_string());
                            overrides.insert(
                                "retrieval.boost.person_name".to_string(),
                                "true".to_string(),
                            );
                            overrides.insert(
                                "retrieval.boost.keyword_overlap".to_string(),
                                "true".to_string(),
                            );
                            overrides.insert(
                                "retrieval.boost.weight.person_name".to_string(),
                                w_pn.to_string(),
                            );
                            overrides.insert(
                                "retrieval.boost.weight.keyword_overlap".to_string(),
                                w_ko.to_string(),
                            );

                            let (r_any, r_all, ndcg, r_any_sess, r_all_sess, lat, _, records) =
                                run_evaluation(
                                    tune_questions,
                                    "hybrid",
                                    Some(overrides),
                                    &target_cache_path,
                                    published,
                                    &note,
                                )
                                .await?;

                            let score = 0.50 * ndcg + 0.40 * r_all + 0.10 * r_any;
                            let detail = format_metrics(
                                r_any, r_all, ndcg, r_any_sess, r_all_sess, &records,
                            );
                            println!(
                                "B: mmr_lambda={}, rerank_pool_size={}, w_person_name={}, w_keyword_overlap={} => score={:.4} ({} Lat={:.2}ms)",
                                mmr, pool, w_pn, w_ko, score, detail, lat
                            );

                            if score > best_score_b {
                                best_score_b = score;
                                winner_b = (*mmr, *pool, *w_pn, *w_ko);
                            }
                        }
                    }
                }
            }

            println!(
                "Winner Phase B: mmr_lambda={}, rerank_pool_size={}, w_person_name={}, w_keyword_overlap={} (Score: {:.4})",
                winner_b.0, winner_b.1, winner_b.2, winner_b.3, best_score_b
            );

            locked_overrides.insert("search.mmr_lambda".to_string(), winner_b.0.to_string());
            locked_overrides.insert(
                "search.rerank_pool_size".to_string(),
                winner_b.1.to_string(),
            );
            locked_overrides.insert(
                "retrieval.boost.person_name".to_string(),
                "true".to_string(),
            );
            locked_overrides.insert(
                "retrieval.boost.keyword_overlap".to_string(),
                "true".to_string(),
            );
            locked_overrides.insert(
                "retrieval.boost.weight.person_name".to_string(),
                winner_b.2.to_string(),
            );
            locked_overrides.insert(
                "retrieval.boost.weight.keyword_overlap".to_string(),
                winner_b.3.to_string(),
            );

            // Phase C: Validation & Fine-Tuning
            println!("--- Phase C: Validation & Fine-Tuning Sweep ---");
            let mut best_score_c = -1.0;
            let mut winner_c = (1.0f32, 0.50f32);

            let ladder_scales = vec![0.5f32, 1.0f32];
            let utility_thresholds = vec![0.45f32, 0.55f32];

            for ls in &ladder_scales {
                for ut in &utility_thresholds {
                    let mut overrides = locked_overrides.clone();
                    overrides.insert("search.ladder_scale".to_string(), ls.to_string());
                    overrides.insert("search.utility_threshold".to_string(), ut.to_string());

                    let (r_any, r_all, ndcg, _, _, lat, _, _) = run_evaluation(
                        tune_questions,
                        "hybrid",
                        Some(overrides),
                        &target_cache_path,
                        published,
                        &note,
                    )
                    .await?;

                    let score = 0.50 * ndcg + 0.40 * r_all + 0.10 * r_any;
                    println!(
                        "C: ladder_scale={}, utility_threshold={} => score={:.4} (R_any={:.4}, nDCG={:.4}, Lat={:.2}ms)",
                        ls, ut, score, r_any, ndcg, lat
                    );

                    if score > best_score_c {
                        best_score_c = score;
                        winner_c = (*ls, *ut);
                    }
                }
            }

            println!(
                "Winner Phase C: ladder_scale={}, utility_threshold={} (Score: {:.4})",
                winner_c.0, winner_c.1, best_score_c
            );

            locked_overrides.insert("search.ladder_scale".to_string(), winner_c.0.to_string());
            locked_overrides.insert(
                "search.utility_threshold".to_string(),
                winner_c.1.to_string(),
            );

            let output_dir = std::path::Path::new("bench_data");
            let _ = std::fs::create_dir_all(output_dir);
            let tuned_params_path = output_dir.join("tuned_params.json");

            let serialized = serde_json::to_string_pretty(&locked_overrides)?;
            std::fs::write(&tuned_params_path, serialized)?;
            println!("Best tuned parameters saved to {:?}", tuned_params_path);
        } else {
            let (
                avg_recall_any_turn,
                avg_recall_all_turn,
                avg_ndcg_turn,
                avg_recall_any_session,
                avg_recall_all_session,
                avg_latency,
                p95_latency,
                records,
            ) = run_evaluation(
                &target_questions,
                &args.mode,
                None,
                &target_cache_path,
                published,
                &note,
            )
            .await?;

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
            println!(
                "Recall_Any@{}:            {:.4}",
                K_RECALL, avg_recall_any_turn
            );
            println!(
                "Recall_All@{}:            {:.4}",
                K_RECALL, avg_recall_all_turn
            );
            println!("nDCG@{}:                  {:.4}", K_NDCG, avg_ndcg_turn);
            println!("-- session granularity (answer_session_ids) --");
            println!(
                "Recall_Any@{} (session):  {:.4}",
                K_RECALL, avg_recall_any_session
            );
            println!(
                "Recall_All@{} (session):  {:.4}",
                K_RECALL, avg_recall_all_session
            );
            println!("--------------------------------------------------------");
            println!("Per-Question-Type R@{} (turn recall_any):", K_NDCG);

            let mut type_counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            let mut type_recall_at10: std::collections::HashMap<String, f32> =
                std::collections::HashMap::new();
            for record in &records {
                *type_counts.entry(record.question_type.clone()).or_insert(0) += 1;
                *type_recall_at10
                    .entry(record.question_type.clone())
                    .or_insert(0.0) += record.recall_any_turn_at10;
            }

            let mut type_keys: Vec<&String> = type_counts.keys().collect();
            type_keys.sort();
            for q_type in &type_keys {
                let count = type_counts[*q_type];
                let avg = type_recall_at10.get(*q_type).cloned().unwrap_or(0.0) / count as f32;
                println!("  - {:<28} (n={:<3}): {:.4}", q_type, count, avg);
            }
            println!("--------------------------------------------------------");
            println!("Category-Specific Metrics:");

            let mut cat_records: std::collections::HashMap<String, Vec<&QuestionResultRecord>> =
                std::collections::HashMap::new();
            for rec in &records {
                let cat = mythrax_core::db::backend::classify_query(&rec.question)
                    .as_str()
                    .to_string();
                cat_records.entry(cat).or_default().push(rec);
            }

            let categories = vec!["preference", "user", "temporal", "default"];
            for cat_name in &categories {
                if let Some(recs) = cat_records.get(*cat_name) {
                    let mut sum_recall_at3 = 0.0;
                    let mut sum_ndcg_at3 = 0.0;
                    let mut rank_values = Vec::new();
                    let mut count_rank_ge5 = 0;

                    for rec in recs {
                        let mut first_rank = None;
                        for (idx, r_id) in rec.retrieved_corpus_ids.iter().enumerate() {
                            if rec.gold_corpus_ids.contains(r_id) {
                                first_rank = Some(idx + 1);
                                break;
                            }
                        }

                        if let Some(rank) = first_rank {
                            rank_values.push(rank as f32);
                            if rank >= 5 {
                                count_rank_ge5 += 1;
                            }
                        } else {
                            count_rank_ge5 += 1;
                        }

                        let r3 = if rec
                            .retrieved_corpus_ids
                            .iter()
                            .take(3)
                            .any(|r_id| rec.gold_corpus_ids.contains(r_id))
                        {
                            1.0
                        } else {
                            0.0
                        };
                        sum_recall_at3 += r3;

                        let mut dcg = 0.0;
                        let mut idcg = 0.0;
                        for i in 0..std::cmp::min(rec.retrieved_corpus_ids.len(), 3) {
                            let gain = if rec.gold_corpus_ids.contains(&rec.retrieved_corpus_ids[i])
                            {
                                1.0
                            } else {
                                0.0
                            };
                            dcg += gain / ((i + 2) as f64).log2();
                        }
                        for i in 0..std::cmp::min(rec.gold_corpus_ids.len(), 3) {
                            idcg += 1.0 / ((i + 2) as f64).log2();
                        }
                        let ndcg3 = if idcg > 0.0 { dcg / idcg } else { 0.0 };
                        sum_ndcg_at3 += ndcg3;
                    }

                    let total = recs.len() as f64;
                    let avg_r3 = (sum_recall_at3 as f64) / total;
                    let avg_ndcg3 = sum_ndcg_at3 / total;
                    let avg_rank = if !rank_values.is_empty() {
                        (rank_values.iter().sum::<f32>() as f64) / rank_values.len() as f64
                    } else {
                        0.0
                    };
                    let freq_ge5 = (count_rank_ge5 as f64) / total;

                    println!("  Category: {}", cat_name);
                    println!("    Recall@3:               {:.4}", avg_r3);
                    println!("    nDCG@3:                 {:.4}", avg_ndcg3);
                    println!("    Avg First Relevant Rank: {:.2}", avg_rank);
                    println!(
                        "    Frequency of Rank >= 5:  {:.4} ({}/{})",
                        freq_ge5,
                        count_rank_ge5,
                        recs.len()
                    );
                } else {
                    println!("  Category: {} (n=0) - No questions", cat_name);
                }
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
            let mut out_file =
                File::create(&output_file_path).context("Failed to create results file")?;
            out_file.write_all((serde_json::to_string(&manifest)? + "\n").as_bytes())?;
            for rec in &records {
                out_file.write_all((serde_json::to_string(rec)? + "\n").as_bytes())?;
            }
            println!("Detailed results written to {:?}", output_file_path);

            if is_published_mode {
                let baseline_path = output_dir.join("BASELINE.md");
                let mut baseline_file =
                    File::create(&baseline_path).context("Failed to create BASELINE.md")?;
                let type_table = {
                    let mut keys: Vec<&String> = type_counts.keys().collect();
                    keys.sort();
                    keys.iter()
                        .map(|q_type| {
                            let count = type_counts[*q_type];
                            let avg = type_recall_at10.get(*q_type).cloned().unwrap_or(0.0)
                                / count as f32;
                            format!(
                                "- **{}** (n={}): R@{} = `{:.4}`",
                                q_type, count, K_NDCG, avg
                            )
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
                    K_RECALL,
                    avg_recall_any_turn,
                    K_RECALL,
                    avg_recall_all_turn,
                    K_NDCG,
                    avg_ndcg_turn,
                    K_RECALL,
                    avg_recall_any_session,
                    K_RECALL,
                    avg_recall_all_session,
                    K_NDCG,
                    type_table,
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

fn extract_entities(text: &str) -> Vec<String> {
    let mut entities = std::collections::HashSet::new();

    // 1. Extract multi-word capitalized phrases (highly reliable proper nouns)
    static MULTI_RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let multi_re = MULTI_RE.get_or_init(|| {
        regex::Regex::new(r"\b[A-Z][a-zA-Z0-9_-]+(?:\s+[A-Z][a-zA-Z0-9_-]+)+\b").unwrap()
    });
    for m in multi_re.find_iter(text) {
        entities.insert(m.as_str().trim().to_string());
    }

    // 2. Extract single-word capitalized proper nouns (excluding first word of each sentence/clause)
    for sentence in text.split(|c| c == '.' || c == '?' || c == '!' || c == '\n') {
        let words: Vec<&str> = sentence.split_whitespace().collect();
        if words.len() > 1 {
            // Skip the first word as it's capitalized due to sentence-start
            for word in &words[1..] {
                // Strip punctuation
                let cleaned: String = word
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                    .collect();
                if !cleaned.is_empty() {
                    let first_char = cleaned.chars().next().unwrap();
                    if first_char.is_ascii_uppercase() {
                        let lower = cleaned.to_lowercase();
                        if !matches!(
                            lower.as_str(),
                            "i" | "the"
                                | "a"
                                | "an"
                                | "we"
                                | "he"
                                | "she"
                                | "they"
                                | "our"
                                | "my"
                                | "it"
                                | "this"
                                | "that"
                                | "you"
                                | "your"
                                | "there"
                                | "here"
                                | "and"
                                | "but"
                                | "or"
                                | "so"
                                | "if"
                                | "then"
                                | "of"
                                | "in"
                                | "on"
                                | "at"
                                | "to"
                                | "for"
                                | "with"
                                | "by"
                        ) {
                            entities.insert(cleaned);
                        }
                    }
                }
            }
        }
    }

    entities.into_iter().collect()
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

    let mut type_counts: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    let mut type_recall_at10: std::collections::HashMap<String, f32> =
        std::collections::HashMap::new();

    unsafe {
        std::env::set_var("MYTHRAX_BENCH", "1");
    }

    let total_q = target_questions.len();
    let mut join_set = tokio::task::JoinSet::new();
    let concurrency_limit = if cfg!(feature = "mlx") { 1 } else { 4 };

    for (q_idx, q) in target_questions.iter().enumerate() {
        println!("Evaluating question {}/{}...", q_idx + 1, total_q);
        while join_set.len() >= concurrency_limit {
            if let Some(res) = join_set.join_next().await {
                let record: QuestionResultRecord =
                    res.context("Parallel evaluation task panicked")??;
                sum_recall_any_turn += record.recall_any_turn_at5;
                sum_recall_all_turn += record.recall_all_turn_at5;
                sum_ndcg_turn += record.ndcg_turn_at10;
                sum_recall_any_session += record.recall_any_session_at5;
                sum_recall_all_session += record.recall_all_session_at5;
                sum_latency += record.query_latency_ms;

                *type_counts.entry(record.question_type.clone()).or_insert(0) += 1;
                *type_recall_at10
                    .entry(record.question_type.clone())
                    .or_insert(0.0) += record.recall_any_turn_at10;

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
            let mut session_user_inputs = std::collections::HashSet::new();

            let mut turn_entities = std::collections::HashMap::new();
            let mut all_entities_set = std::collections::HashSet::new();

            for (sess_idx, session_id) in q.haystack_session_ids.iter().enumerate() {
                if let Some(session_turns) = q.haystack_sessions.get(sess_idx) {
                    for (turn_idx, turn) in session_turns.iter().enumerate() {
                        let corpus_id = format!("{}_turn_{}", session_id, turn_idx);
                        let role_lower = turn.role.to_lowercase();
                        
                        if role_lower == "user" {
                            let norm_content = turn.content.to_lowercase().replace("favourite", "favorite");
                            let clean_stm_value = |val: &str| -> String {
                                let trimmed = val.trim();
                                let mut cleaned = trimmed;
                                if cleaned.starts_with("the ") {
                                    cleaned = &cleaned[4..];
                                } else if cleaned.starts_with("a ") {
                                    cleaned = &cleaned[2..];
                                } else if cleaned.starts_with("an ") {
                                    cleaned = &cleaned[3..];
                                }
                                cleaned.trim().to_string()
                            };

                            for sentence in norm_content.split('.') {
                                let sentence = sentence.trim();
                                if sentence.is_empty() {
                                    continue;
                                }

                                // 1. degree
                                if let Some(idx) = sentence.find("degree in") {
                                    let val = clean_stm_value(&sentence[idx + "degree in".len()..]);
                                    if !val.is_empty() {
                                        let _ = backend.save_stm(session_id, "degree", &val).await;
                                    }
                                } else if let Some(idx) = sentence.find("majored in") {
                                    let val = clean_stm_value(&sentence[idx + "majored in".len()..]);
                                    if !val.is_empty() {
                                        let _ = backend.save_stm(session_id, "degree", &val).await;
                                    }
                                }

                                // 2. favorite
                                if let Some(fav_idx) = sentence.find("favorite ") {
                                    let remaining = &sentence[fav_idx + "favorite ".len()..];
                                    if let Some(is_idx) = remaining.find(" is ") {
                                        let word = remaining[..is_idx].trim();
                                        if !word.is_empty() && word.chars().all(|c| c.is_alphanumeric() || c == '_') {
                                            let val = clean_stm_value(&remaining[is_idx + " is ".len()..]);
                                            if !val.is_empty() {
                                                let key = format!("favorite_{}", word);
                                                let _ = backend.save_stm(session_id, &key, &val).await;
                                            }
                                        }
                                    }
                                }

                                // 3. prefer
                                if let Some(idx) = sentence.find("prefer") {
                                    let val = clean_stm_value(&sentence[idx + "prefer".len()..]);
                                    if !val.is_empty() {
                                        let _ = backend.save_stm(session_id, "preference", &val).await;
                                    }
                                }

                                // 4. booked (chose, selected, booked)
                                let mut booked_idx = None;
                                if let Some(idx) = sentence.find("chose") {
                                    booked_idx = Some((idx, "chose".len()));
                                } else if let Some(idx) = sentence.find("selected") {
                                    booked_idx = Some((idx, "selected".len()));
                                } else if let Some(idx) = sentence.find("booked") {
                                    booked_idx = Some((idx, "booked".len()));
                                }
                                if let Some((idx, len)) = booked_idx {
                                    let val = clean_stm_value(&sentence[idx + len..]);
                                    if !val.is_empty() {
                                        let _ = backend.save_stm(session_id, "booked", &val).await;
                                    }
                                }

                                // 5. occupation (work as a, work at)
                                let mut occ_idx = None;
                                if let Some(idx) = sentence.find("work as a") {
                                    occ_idx = Some((idx, "work as a".len()));
                                } else if let Some(idx) = sentence.find("work at") {
                                    occ_idx = Some((idx, "work at".len()));
                                }
                                if let Some((idx, len)) = occ_idx {
                                    let val = clean_stm_value(&sentence[idx + len..]);
                                    if !val.is_empty() {
                                        let _ = backend.save_stm(session_id, "occupation", &val).await;
                                    }
                                }

                                // 6. garden/homegrown (harvested, garden)
                                if let Some(idx) = sentence.find("harvested") {
                                    let val = clean_stm_value(&sentence[idx + "harvested".len()..]);
                                    if !val.is_empty() {
                                        let _ = backend.save_stm(session_id, "harvested", &val).await;
                                    }
                                } else if let Some(idx) = sentence.find("garden") {
                                    let val = clean_stm_value(&sentence[idx + "garden".len()..]);
                                    if !val.is_empty() {
                                        let _ = backend.save_stm(session_id, "garden", &val).await;
                                    }
                                }
                            }
                        }

                        let ents = extract_entities(&turn.content);
                        if !ents.is_empty() {
                            for ent in &ents {
                                all_entities_set.insert(ent.clone());
                            }
                            turn_entities.insert(corpus_id.clone(), ents);
                        }

                        let node_type = match role_lower.as_str() {
                            "user" => {
                                if session_user_inputs.insert(session_id.clone()) {
                                    "user_input".to_string()
                                } else {
                                    "user_feedback".to_string()
                                }
                            }
                            "assistant" => "agent_thought".to_string(),
                            "system" => "system_log".to_string(),
                            "tool" | "computer" | "tool_result" => "tool_execution".to_string(),
                            _ => "agent_thought".to_string(),
                        };
                        let ep = EpisodeSave {
                            title: format!("Session {} - Turn {}", session_id, turn_idx),
                            content: format!("{}: {}", turn.role, turn.content),
                            scope: Some("general".to_string()),
                            vault_path: Some(corpus_id.clone()),
                            session_id: Some(session_id.clone()),
                            node_type: Some(node_type),
                            ..Default::default()
                        };
                        episodes_to_ingest.push(ep);
                    }
                }
            }
            backend.save_episodes_batch(&episodes_to_ingest).await
                .context("Failed to batch ingest haystack turns")?;

            let surreal_backend = backend.as_any().downcast_ref::<SurrealBackend>().unwrap();
            let db = &surreal_backend.db;

            let mut corpus_to_ep_id = std::collections::HashMap::new();
            let mut ep_response = db.query("SELECT id, vault_path FROM episode;").await?;
            #[derive(serde::Deserialize, surrealdb_types::SurrealValue, Debug)]
            struct EpResult {
                id: surrealdb::types::RecordId,
                vault_path: Option<String>,
            }
            let ep_results: Vec<EpResult> = ep_response.take(0).unwrap_or_default();
            for r in ep_results {
                if let Some(vp) = r.vault_path {
                    corpus_to_ep_id.insert(vp, mythrax_core::db::backend::format_record_id(&r.id));
                }
            }

            let mut transaction_sql = "BEGIN TRANSACTION;".to_string();
            for ent in all_entities_set {
                let escaped = ent.replace("'", "\\'");
                transaction_sql.push_str(&format!(
                    "UPSERT entity:⟨{}⟩ CONTENT {{ name: '{}', entity_type: 'concept', summary: '', labels: ['concept'], scope: 'general' }}; ",
                    escaped, escaped
                ));
            }

            for (corpus_id, entities) in &turn_entities {
                if let Some(ep_id) = corpus_to_ep_id.get(corpus_id) {
                    let ep_uuid = ep_id.strip_prefix("episode:").unwrap_or(ep_id);
                    for ent in entities {
                        let escaped = ent.replace("'", "\\'");
                        transaction_sql.push_str(&format!(
                            "RELATE episode:⟨{}⟩ -> mentions -> entity:⟨{}⟩ CONTENT {{ created_at: time::now() }}; ",
                            ep_uuid, escaped
                        ));
                    }
                }
            }

            for sess_idx in 0..q.haystack_session_ids.len() {
                let session_id = &q.haystack_session_ids[sess_idx];
                if let Some(turns) = q.haystack_sessions.get(sess_idx) {
                    for i in 0..turns.len() {
                        let corpus_id_a = format!("{}_turn_{}", session_id, i);
                        let ents_a = match turn_entities.get(&corpus_id_a) {
                            Some(v) => v,
                            None => continue,
                        };
                        for j in (i + 1)..turns.len() {
                            let corpus_id_b = format!("{}_turn_{}", session_id, j);
                            let ents_b = match turn_entities.get(&corpus_id_b) {
                                Some(v) => v,
                                None => continue,
                            };
                            let has_intersection = ents_a.iter().any(|e| ents_b.contains(e));
                            if has_intersection {
                                if let (Some(ep_a), Some(ep_b)) = (corpus_to_ep_id.get(&corpus_id_a), corpus_to_ep_id.get(&corpus_id_b)) {
                                    let ep_a_uuid = ep_a.strip_prefix("episode:").unwrap_or(ep_a);
                                    let ep_b_uuid = ep_b.strip_prefix("episode:").unwrap_or(ep_b);
                                    transaction_sql.push_str(&format!(
                                        "RELATE episode:⟨{}⟩ -> relates_to -> episode:⟨{}⟩ CONTENT {{ confidence: 0.85 }}; ",
                                        ep_a_uuid, ep_b_uuid
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            transaction_sql.push_str("COMMIT TRANSACTION;");
            let _ = db.query(&transaction_sql).await?.check();

            // SurrealDB in-memory FTS index updates are synchronous on commit; no sleep needed

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
            let active_session_id = q.answer_session_ids.first()
                .map(|s| s.as_str())
                .or_else(|| q.haystack_session_ids.last().map(|s| s.as_str()));
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
                    active_session_id, // session_id
                    true,        // include_archived
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
                question: q.question.clone(),
                category: mythrax_core::db::backend::classify_query(&q.question).as_str().to_string(),
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
        *type_recall_at10
            .entry(record.question_type.clone())
            .or_insert(0.0) += record.recall_any_turn_at10;

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
                println!(
                    "Downloading... {:.2} MB",
                    downloaded as f64 / (1024.0 * 1024.0)
                );
            }
            last_reported = downloaded;
        }
    }
    println!(
        "Finished downloading ({:.2} MB)",
        downloaded as f64 / (1024.0 * 1024.0)
    );
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
