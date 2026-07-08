use anyhow::{Result, Context};
use serde::Deserialize;
use std::fs::File;
use std::io::Read;
use std::collections::HashSet;
use std::sync::Arc;
use mythrax_core::db::{SurrealBackend, StorageBackend};
use mythrax_core::embeddings::{cache_embedding, save_embedding_cache_to_disk, load_embedding_cache_from_disk};

#[derive(Debug, Clone, Deserialize)]
struct QuestionEntry {
    question: String,
    haystack_session_ids: Vec<String>,
    haystack_sessions: Vec<Vec<TurnEntry>>,
}

#[derive(Debug, Clone, Deserialize)]
struct TurnEntry {
    role: String,
    content: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("Starting parallel embedding cache pre-builder...");

    // Find the dataset file
    let path = std::path::Path::new("bench_data/official/longmemeval_s_cleaned.json");
    let path_parent = std::path::Path::new("../bench_data/official/longmemeval_s_cleaned.json");
    let dataset_path = if path.exists() {
        path
    } else if path_parent.exists() {
        path_parent
    } else {
        path
    };

    println!("Loading dataset from {:?}", dataset_path);
    let mut file = File::open(dataset_path).context("Failed to open dataset file")?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let questions: Vec<QuestionEntry> = serde_json::from_str(&contents)?;
    println!("Loaded {} questions.", questions.len());

    // Extract all unique texts to embed
    let mut unique_texts = HashSet::new();
    let cleaning_re = regex::Regex::new(r"\b(before|preceding|previously|prior|earlier|ago|last|after|following|subsequently|later|next|recent|recently|latest|newest|today|now)\b").unwrap();
    for q in &questions {
        unique_texts.insert(q.question.clone());
        unique_texts.insert(format!("search_query: {}", q.question));
        let cleaned = cleaning_re.replace_all(&q.question, "").to_string();
        let cleaned_query = cleaned.split_whitespace().collect::<Vec<&str>>().join(" ");
        unique_texts.insert(format!("search_query: {}", cleaned_query));
        for (sess_idx, session_id) in q.haystack_session_ids.iter().enumerate() {
            if let Some(session_turns) = q.haystack_sessions.get(sess_idx) {
                for (turn_idx, turn) in session_turns.iter().enumerate() {
                    let title = format!("Session {} - Turn {}", session_id, turn_idx);
                    let content = format!("{}: {}", turn.role, turn.content);
                    unique_texts.insert(format!("{}: {}", title, content));
                }
            }
        }
    }
    
    let total_unique = unique_texts.len();
    println!("Found {} unique turns to embed.", total_unique);

    // Load existing cache if any to avoid re-embedding
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
    let target_cache_path = target_cache_path.as_path();

    if target_cache_path.exists() {
        println!("Loading existing embedding cache from {:?}", target_cache_path);
        let _ = load_embedding_cache_from_disk(target_cache_path);
    }

    // Filter out already cached texts
    let mut texts_to_embed = Vec::new();
    for text in unique_texts {
        if mythrax_core::embeddings::get_cached_embedding(&text).is_none() {
            texts_to_embed.push(text);
        }
    }

    // Sort by length to minimize token padding per batch (bucketed batching)
    texts_to_embed.sort_by_key(|t| t.len());

    let to_embed_count = texts_to_embed.len();
    println!("{} turns already cached. {} turns need embedding.", total_unique - to_embed_count, to_embed_count);

    if to_embed_count == 0 {
        println!("All turns are already embedded and cached!");
        return Ok(());
    }

    // Initialize backend/embedder
    println!("Initializing SurrealDB in-memory engine and loading model...");
    let backend = Arc::new(SurrealBackend::new_in_memory().await.context("Failed to init backend")?);

    // Batch embed with parallel tasks
    let batch_size = 512;
    let concurrency_limit = 4;
    let mut join_set = tokio::task::JoinSet::new();
    
    let total_chunks = (to_embed_count + batch_size - 1) / batch_size;
    let mut chunk_iter = texts_to_embed.chunks(batch_size);
    let mut active = 0;
    let mut completed = 0;

    println!("Spawning parallel embedding workers (concurrency limit: {})...", concurrency_limit);

    loop {
        // Fill up parallel queue
        while active < concurrency_limit {
            if let Some(chunk) = chunk_iter.next() {
                let chunk_vec = chunk.to_vec();
                let backend_clone = backend.clone();
                join_set.spawn(async move {
                    let embeddings = backend_clone.embed_batch(&chunk_vec).await?;
                    Ok::<_, anyhow::Error>((chunk_vec, embeddings))
                });
                active += 1;
            } else {
                break;
            }
        }

        if active == 0 {
            break;
        }

        // Wait for one to finish and process results
        if let Some(res) = join_set.join_next().await {
            active -= 1;
            completed += 1;
            
            let (chunk_vec, embeddings) = res.context("Parallel embedding task panicked")??;
            
            for (idx, text) in chunk_vec.iter().enumerate() {
                cache_embedding(text.clone(), embeddings[idx].clone());
            }

            println!("Completed batch {}/{} (size {})...", completed, total_chunks, chunk_vec.len());
            
            // Periodically save to disk
            if completed % 10 == 0 || completed == total_chunks {
                save_embedding_cache_to_disk(target_cache_path).context("Failed to save cache to disk")?;
            }
        }
    }

    // Final save
    save_embedding_cache_to_disk(target_cache_path).context("Failed to save cache to disk")?;
    println!("Embedding complete! Cache successfully written to {:?}", target_cache_path);
    Ok(())
}
