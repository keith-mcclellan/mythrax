use mythrax_core::db::backend::{SurrealBackend, StorageBackend};
use mythrax_core::embeddings::load_embedding_cache_from_disk;
use std::path::Path;

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0;
    let mut a_norm = 0.0;
    let mut b_norm = 0.0;
    for i in 0..768 {
        dot += a[i] * b[i];
        a_norm += a[i] * a[i];
        b_norm += b[i] * b[i];
    }
    dot / (a_norm.sqrt() * b_norm.sqrt())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cache_path = Path::new("embedding_cache.bin");
    load_embedding_cache_from_disk(cache_path)?;
    println!("Loaded embedding cache.");

    let backend = SurrealBackend::new("surrealkv://bench_data/mythrax_bench.db").await?;
    backend.init().await?;
    println!("Backend initialized.");

    // Generate query embedding
    let embedder = mythrax_core::embeddings::LocalEmbedder::get_global()?;
    let q_emb = embedder.embed("search_query: How long did I wait for the decision on my asylum application?")?;
    println!("Query embedding generated.");

    // Target
    let mut res = backend.db.query("SELECT VALUE embedding FROM episode WHERE vault_path = 'answer_530960c1_turn_4';").await?;
    let target_emb: Option<Vec<f32>> = res.take(0)?;

    if let Some(t) = target_emb {
        println!("Cosine Similarity to target (answer_530960c1_turn_4): {}", cosine_similarity(&q_emb, &t));
    } else {
        println!("Target has no embedding in DB!");
    }

    println!("--- RUNNING BACKEND SEARCH ---");
    let search_res = backend.search("How long did I wait for the decision on my asylum application?", Some("general"), false, 10, 0, 0.0, None, false, true, true).await?;
    println!("Found {} results:", search_res.results.len());
    for (i, r) in search_res.results.iter().enumerate() {
        println!("{}. id: {}, title: {}, similarity: {}", i + 1, r.id, r.title, r.similarity);
    }

    Ok(())
}
