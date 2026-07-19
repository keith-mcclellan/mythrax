use anyhow::Result;
use std::collections::HashMap;
use surrealdb_types::SurrealValue;

// A simple DBSCAN implementation matching the codebase's signature
fn simulate_dbscan(
    embeddings: &[&[f32]],
    eps: f32,
    min_samples: usize,
) -> Vec<Option<usize>> {
    let n = embeddings.len();
    let mut labels = vec![None; n];
    let mut cluster_id = 0;

    let mut visited = vec![false; n];

    for i in 0..n {
        if visited[i] {
            continue;
        }
        visited[i] = true;

        let neighbors = find_neighbors(embeddings, i, eps);
        if neighbors.len() < min_samples {
            // Label as outlier (None)
            continue;
        }

        labels[i] = Some(cluster_id);
        expand_cluster(embeddings, &mut labels, i, &neighbors, cluster_id, eps, min_samples, &mut visited);
        cluster_id += 1;
    }

    labels
}

fn find_neighbors(embeddings: &[&[f32]], index: usize, eps: f32) -> Vec<usize> {
    let mut neighbors = Vec::new();
    let query = embeddings[index];
    for (i, &emb) in embeddings.iter().enumerate() {
        let dist = cosine_distance(query, emb);
        if dist <= eps {
            neighbors.push(i);
        }
    }
    neighbors
}

fn expand_cluster(
    embeddings: &[&[f32]],
    labels: &mut [Option<usize>],
    _core_index: usize,
    neighbors: &[usize],
    cluster_id: usize,
    eps: f32,
    min_samples: usize,
    visited: &mut [bool],
) {
    let mut queue = neighbors.to_vec();
    let mut i = 0;
    while i < queue.len() {
        let p = queue[i];
        if !visited[p] {
            visited[p] = true;
            let p_neighbors = find_neighbors(embeddings, p, eps);
            if p_neighbors.len() >= min_samples {
                for &pn in &p_neighbors {
                    if !queue.contains(&pn) {
                        queue.push(pn);
                    }
                }
            }
        }
        if labels[p].is_none() {
            labels[p] = Some(cluster_id);
        }
        i += 1;
    }
}

fn cosine_distance(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }
    if norm_a == 0.0 || norm_b == 0.0 {
        return 1.0;
    }
    let sim = dot / (norm_a.sqrt() * norm_b.sqrt());
    (1.0 - sim).max(0.0)
}

#[tokio::main]
async fn main() -> Result<()> {
    let home = std::env::var("HOME")?;
    let db_path = format!("surrealkv://{}/.mythrax/db", home);
    println!("Connecting to SurrealDB database at: {}", db_path);

    // Initialize SurrealDB in-process
    let db = surrealdb::engine::any::connect(&db_path).await?;
    db.use_ns("mythrax").use_db("memory").await?;

    // Query all episodes
    let mut response = db.query("SELECT id, scope, title, embedding FROM episode;").await?;
    
    #[derive(serde::Deserialize, Debug, surrealdb_types::SurrealValue)]
    struct EpisodeRow {
        scope: Option<String>,
        embedding: Option<Vec<f32>>,
    }

    let episodes: Vec<EpisodeRow> = response.take(0)?;
    println!("Retrieved {} episodes from database.", episodes.len());

    let mut scope_embeddings: HashMap<String, Vec<Vec<f32>>> = HashMap::new();
    for ep in episodes {
        if let Some(emb) = ep.embedding {
            let scope = ep.scope.unwrap_or_else(|| "general".to_string());
            scope_embeddings.entry(scope).or_default().push(emb);
        }
    }

    println!("\n### 📊 Clustering Simulation Report");
    println!("| Scope | Epsilon | Clusters | Outliers | Avg Cluster Size | Max Cluster Size |");
    println!("|---|---|---|---|---|---|");

    let eps_values = vec![0.08, 0.10, 0.12, 0.15, 0.18, 0.22, 0.25];

    let mut scopes: Vec<String> = scope_embeddings.keys().cloned().collect();
    scopes.sort();

    for scope in &scopes {
        let embs = &scope_embeddings[scope];
        if embs.is_empty() {
            continue;
        }

        let emb_refs: Vec<&[f32]> = embs.iter().map(|v| v.as_slice()).collect();

        for &eps in &eps_values {
            let labels = simulate_dbscan(&emb_refs, eps, 2);
            
            let mut cluster_sizes: HashMap<usize, usize> = HashMap::new();
            let mut outlier_count = 0;

            for label in labels {
                if let Some(lbl) = label {
                    *cluster_sizes.entry(lbl).or_default() += 1;
                } else {
                    outlier_count += 1;
                }
            }

            let num_clusters = cluster_sizes.len();
            let max_cluster = cluster_sizes.values().max().cloned().unwrap_or(0);
            let avg_cluster = if num_clusters > 0 {
                let sum: usize = cluster_sizes.values().sum();
                (sum as f32) / (num_clusters as f32)
            } else {
                0.0
            };

            println!(
                "| `{}` | `{:.2}` | **{}** | **{}** | `{:.1}` | **{}** |",
                scope, eps, num_clusters, outlier_count, avg_cluster, max_cluster
            );
        }
    }

    Ok(())
}
