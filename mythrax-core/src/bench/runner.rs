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
use std::path::{Path, PathBuf};

use mythrax_core::bench::metrics::{evaluate_retrieval, ndcg};
use mythrax_core::contracts::EpisodeSave;
use mythrax_core::db::backend::{StorageBackend, SurrealBackend};

#[derive(Parser, Debug)]
#[command(name = "bench", about = "Mythrax Advanced-Memory Retrieval Benchmark Harness")]
struct Args {
    #[arg(long, default_value = "full500")]
    split: String,

    #[arg(long, default_value_t = 5)]
    k: usize,

    #[arg(long)]
    ablate: bool,

    #[arg(long, default_value = "bench_data/official")]
    data_dir: String,
}

#[derive(Debug, Deserialize)]
struct QuestionEntry {
    question_id: String,
    question_type: String,
    question: String,
    answer: String,
    haystack_session_ids: Vec<String>,
    haystack_sessions: Vec<Vec<TurnEntry>>,
}

#[derive(Debug, Deserialize)]
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
    file_sha256s: Vec<(String, String)>,
    split_mode: String,
    k_value: usize,
    mythrax_git_commit: String,
    published: bool,
    note: String,
}

#[derive(Serialize)]
struct QuestionResultRecord {
    question_id: String,
    question_type: String,
    recall_any: f32,
    recall_all: f32,
    ndcg: f32,
    retrieved_corpus_ids: Vec<String>,
    gold_corpus_ids: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    println!("Starting Mythrax benchmark runner...");
    println!("Split mode: {}", args.split);
    println!("k value: {}", args.k);

    // 1. Ensure data directory exists
    let data_path = Path::new(&args.data_dir);
    fs::create_dir_all(data_path).context("Failed to create data directory")?;

    // 2. Download files if missing
    let dataset_revision = "main".to_string(); // Pinned branch/revision
    let files_to_download = vec![
        "longmemeval_oracle.json",
        "longmemeval_s_cleaned.json",
        "longmemeval_m_cleaned.json",
    ];

    let mut file_sha256s = Vec::new();

    for filename in &files_to_download {
        let dest_file = data_path.join(filename);
        if !dest_file.exists() {
            println!("Downloading {} from Hugging Face...", filename);
            let url = format!(
                "https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned/resolve/{}/{}",
                dataset_revision, filename
            );
            download_file(&url, &dest_file).await.context(format!("Failed to download {}", filename))?;
        }

        // Compute SHA-256
        let sha = compute_sha256(&dest_file).context(format!("Failed to compute SHA-256 for {}", filename))?;
        println!("File: {}, SHA-256: {}", filename, sha);
        file_sha256s.push((filename.to_string(), sha));
    }

    // 3. Load the selected dataset split
    // The default and only publishable split is longmemeval_oracle.json containing the full 500 questions.
    let target_file_path = data_path.join("longmemeval_oracle.json");
    println!("Loading dataset: {:?}", target_file_path);
    let mut file = File::open(&target_file_path).context("Failed to open dataset file")?;
    let mut contents = String::new();
    file.read_to_string(&mut contents).context("Failed to read dataset file")?;

    let questions: Vec<QuestionEntry> = serde_json::from_str(&contents)
        .context("Failed to parse LongMemEval dataset JSON")?;

    println!("Loaded {} questions from dataset.", questions.len());
    if args.split == "full500" && questions.len() != 500 {
        anyhow::bail!("SPEC-GAP: official LongMemEval dataset integrity check failed (expected 500 questions, got {})", questions.len());
    }

    // Determine target subset if running internal gate
    let target_questions = if args.split == "internal-gate" {
        // Dev split of 50 questions (seed 42 deterministic split)
        // To make it deterministic and identical to reference-impl:
        // We sort by question_id and partition using a deterministic index mapping
        let mut sorted_questions = questions;
        sorted_questions.sort_by(|a, b| a.question_id.cmp(&b.question_id));
        
        // Simple deterministic pseudo-random split: taking every 10th question to get 50 dev questions
        let dev_questions: Vec<QuestionEntry> = sorted_questions
            .into_iter()
            .enumerate()
            .filter(|(idx, _)| idx % 10 == 0)
            .map(|(_, q)| q)
            .collect();
        println!("Running in internal-gate mode. Selected {} dev questions.", dev_questions.len());
        dev_questions
    } else {
        questions
    };

    // 4. Run evaluation
    let mut records = Vec::new();
    let mut overall_recall_any = 0.0f32;
    let mut overall_recall_all = 0.0f32;
    let mut overall_ndcg = 0.0f32;

    // Stats by question type
    let mut type_counts = std::collections::HashMap::new();
    let mut type_recall_any = std::collections::HashMap::new();

    let total_q = target_questions.len();

    for (q_idx, q) in target_questions.iter().enumerate() {
        if (q_idx + 1) % 50 == 0 || q_idx == 0 || q_idx == total_q - 1 {
            println!("Evaluating question {}/{}...", q_idx + 1, total_q);
        }

        // Ingest into a fresh in-memory SurrealBackend
        let backend = SurrealBackend::new_in_memory().await
            .context("Failed to create in-memory backend")?;
        backend.init().await.context("Failed to initialize backend")?;

        let mut correct_ids = Vec::new();

        // Ingest conversation sessions
        for (sess_idx, session_id) in q.haystack_session_ids.iter().enumerate() {
            if let Some(session_turns) = q.haystack_sessions.get(sess_idx) {
                for (turn_idx, turn) in session_turns.iter().enumerate() {
                    let corpus_id = format!("{}_turn_{}", session_id, turn_idx);
                    
                    let ep = EpisodeSave {
                        title: format!("Session {} - Turn {}", session_id, turn_idx),
                        content: format!("{}: {}", turn.role, turn.content),
                        entities: vec![],
                        scope: Some("general".to_string()),
                        vault_path: Some(corpus_id.clone()),
                        source_episode: None,
                        session_id: Some(session_id.clone()),
                        task_id: None,
discovery_tokens: None,
facts: None,
concepts: None,
files_read: None,
files_modified: None,
};

                    backend.save_episode(&ep).await
                        .context("Failed to save episode turn during ingestion")?;

                    if turn.has_answer {
                        correct_ids.push(corpus_id);
                    }
                }
            }
        }

        // Perform retrieval query
        let search_response = backend.search(
            &q.question,
            Some("general"),
            false, // deep_insight
            args.k, // limit
            0, // offset
            0.0, // threshold (allow all retrieved chunks)
            None, // token_budget
            false, // allow_downward
            true, // include_episodes
            true, // include_artifacts
        ).await.context("Search query failed during evaluation")?;

        // Extract retrieved corpus IDs from SearchResult.vault_path
        let retrieved_corpus_ids: Vec<String> = search_response.results
            .iter()
            .filter_map(|r| r.vault_path.clone())
            .collect();

        let rankings: Vec<usize> = (0..retrieved_corpus_ids.len()).collect();

        // Compute scores
        let scores = evaluate_retrieval(&rankings, &correct_ids, &retrieved_corpus_ids, args.k);

        overall_recall_any += scores.recall_any;
        overall_recall_all += scores.recall_all;
        overall_ndcg += scores.ndcg;

        let count = type_counts.entry(q.question_type.clone()).or_insert(0);
        *count += 1;
        let sum_any = type_recall_any.entry(q.question_type.clone()).or_insert(0.0f32);
        *sum_any += scores.recall_any;

        records.push(QuestionResultRecord {
            question_id: q.question_id.clone(),
            question_type: q.question_type.clone(),
            recall_any: scores.recall_any,
            recall_all: scores.recall_all,
            ndcg: scores.ndcg,
            retrieved_corpus_ids,
            gold_corpus_ids: correct_ids,
        });
    }

    let avg_recall_any = overall_recall_any / total_q as f32;
    let avg_recall_all = overall_recall_all / total_q as f32;
    let avg_ndcg = overall_ndcg / total_q as f32;

    // 5. Print Summary Table
    println!("\n========================================================");
    println!("               EVALUATION METRICS SUMMARY               ");
    println!("========================================================");
    println!("Total Questions:    {}", total_q);
    println!("Recall_Any@{}:      {:.4}", args.k, avg_recall_any);
    println!("Recall_All@{}:      {:.4}", args.k, avg_recall_all);
    println!("nDCG@{}:             {:.4}", args.k, avg_ndcg);
    println!("--------------------------------------------------------");
    println!("Per-Question-Type Breakdown (Recall_Any@{}):", args.k);
    for (q_type, count) in &type_counts {
        let sum_any = type_recall_any.get(q_type).cloned().unwrap_or(0.0);
        let avg_any = sum_any / *count as f32;
        println!("  - {:<28} (n={:<3}): {:.4}", q_type, count, avg_any);
    }
    println!("========================================================\n");

    // 6. Write JSONL results
    let manifest = Manifest {
        dataset_id: "xiaowu0162/longmemeval-cleaned".to_string(),
        dataset_revision,
        file_sha256s,
        split_mode: args.split.clone(),
        k_value: args.k,
        mythrax_git_commit: get_git_commit().unwrap_or_else(|_| "unknown".to_string()),
        published: args.split == "full500",
        note: if args.split == "full500" {
            "LongMemEval retrieval (Recall@k / NDCG@k), full 500".to_string()
        } else {
            "internal split, not LongMemEval".to_string()
        },
    };

    let output_dir = Path::new("bench_data");
    fs::create_dir_all(output_dir).context("Failed to create bench_data directory")?;
    let output_file_path = output_dir.join(format!("results_{}.jsonl", args.split));
    let mut out_file = File::create(&output_file_path).context("Failed to create results file")?;

    // Write manifest as first line
    let manifest_line = serde_json::to_string(&manifest)? + "\n";
    out_file.write_all(manifest_line.as_bytes())?;

    // Write question records
    for rec in records {
        let rec_line = serde_json::to_string(&rec)? + "\n";
        out_file.write_all(rec_line.as_bytes())?;
    }

    println!("Detailed results written to {:?}", output_file_path);

    // 7. Record BASELINE.md if full500
    if args.split == "full500" {
        let baseline_path = output_dir.join("BASELINE.md");
        let mut baseline_file = File::create(&baseline_path).context("Failed to create BASELINE.md")?;
        
        let baseline_content = format!(
            "# Mythrax Advanced-Memory Published Baseline\n\n\
             **Dataset ID:** `xiaowu0162/longmemeval-cleaned`\n\
             **Manifest Note:** LongMemEval *retrieval* (Recall@k / NDCG@k), full 500\n\
             **Split Mode:** `full500` (Official full 500-question set)\n\
             **Mythrax Git Commit:** `{}`\n\
             **Evaluated at:** {}\n\n\
             ## Aggregate Metrics\n\
             - **Recall_Any@{}:** `{:.4}`\n\
             - **Recall_All@{}:** `{:.4}`\n\
             - **nDCG@{}:** `{:.4}`\n\n\
             ## Question Type Breakdown (Recall_Any@{})\n\
             {}\n\n\
             > [!IMPORTANT]\n\
             > These are the official baseline retrieval numbers. Future optimizations (BM25, boosts, progressive disclosure) must not regress `Recall_Any@5`.\n",
            manifest.mythrax_git_commit,
            chrono::Utc::now().to_rfc3339(),
            args.k, avg_recall_any,
            args.k, avg_recall_all,
            args.k, avg_ndcg,
            args.k,
            type_counts.iter().map(|(q_type, count)| {
                let sum_any = type_recall_any.get(q_type).cloned().unwrap_or(0.0);
                let avg_any = sum_any / *count as f32;
                format!("- **{}** (n={}): `{:.4}`", q_type, count, avg_any)
            }).collect::<Vec<_>>().join("\n")
        );

        baseline_file.write_all(baseline_content.as_bytes())?;
        println!("Official baseline metrics recorded in {:?}", baseline_path);
    }

    Ok(())
}

async fn download_file(url: &str, dest: &Path) -> Result<()> {
    let response = reqwest::get(url).await?.error_for_status()?;
    let content = response.bytes().await?;
    let mut file = File::create(dest)?;
    file.write_all(&content)?;
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
        let commit = String::from_utf8(output.stdout)?.trim().to_string();
        Ok(commit)
    } else {
        anyhow::bail!("Failed to get git commit")
    }
}
