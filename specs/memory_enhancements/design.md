# Design - Memory Engine Enhancements

## Overview
This design details the backend database schema updates, embedding generation rules, ranking score blending formulas, graph relations, and watcher integration required to close all memory engine gaps.

## Execution Flow

### 1. Vector Search Flow
When an agent or user queries memory:
1.  Try to compute the query embedding using `LocalEmbedder::embed`.
2.  If successful:
    - Run the SurrealDB KNN query to select the top 100 candidates:
      `SELECT ... WHERE embedding <|100|> $query_embedding`
    - Calculate dot product similarity $S$ in Rust for all candidates (as embeddings are L2 normalized).
    - Blending: Compute $\text{Final Score} = S \times (0.7 + 0.3 \times \text{utility\_score})$.
    - Filter: Retain only candidates where $\text{Final Score} \ge \text{threshold}$ (defaults to `0.55`).
    - Sort the filtered list in descending order of the blended score.
    - Calculate pagination metrics:
      * `total_matches` = number of filtered candidates.
      * `has_more` = `total_matches > offset + limit`.
      * `next_offset` = `offset + limit`.
    - Slice: Return the window `results[offset .. std::cmp::min(offset + limit, total_matches)]`.
3.  If embedding fails or is unavailable:
    - Fall back to substring match search.
    - Similarity defaults to `1.0`. Filter by threshold, compute pagination metrics, and slice.

### 2. MCP Notification & Slicing
1.  The Axum `/v1/search` endpoint returns the strongly-typed `SearchResponse` JSON object.
2.  The MCP `search_memories` tool takes the results and formats the output string.
3.  If `has_more` is `true`, the MCP server appends a human-readable metadata footer to the text block:
    `"\n\n=== PAGINATION NOTICE: There are {remainder} more matching memories. To retrieve the next page, query search_memories with offset={next_offset} and limit={limit}. ==="`

### 3. dreaming & Compaction Flow
During background dreaming or compaction:
1.  **DBSCAN/Centroids**: Episodes are grouped into wiki insight nodes.
2.  **Insight Save**: Save `WikiNode` to the database using `save_wiki_node`. The embedder automatically calculates embeddings over `"{name}: {content}"`.
3.  **Relate Episodes**: Run relation queries:
    `RELATE $episode_id -> relates_to -> $wiki_node_id UNIQUE;`
4.  **Relate Wisdom**: Relate rules:
    `RELATE $episode_id -> relates_to -> $wisdom_id UNIQUE;`
5.  **Compaction Relate**: Connect compaction node to its source insights:
    `RELATE $insight_id -> relates_to -> $compaction_id UNIQUE;`
6.  **Global Relate**: Connect compaction nodes to global compaction:
    `RELATE $compaction_id -> relates_to -> $global_compaction_id UNIQUE;`

### 4. File Deletion Sync Flow
1.  The notify watcher receives a remove event (`is_remove`) for a markdown file in the vault.
2.  The watcher strip prefixes the absolute path and queries `delete_by_vault_path(rel_path)` on the storage backend.
3.  The database executes deletion queries on all cache tables:
    `DELETE FROM episode WHERE vault_path = $path;`
    `DELETE FROM wisdom WHERE vault_path = $path;`
    `DELETE FROM wiki_node WHERE vault_path = $path;`

### 5. Automatic Startup Reprocessing
1.  On backend daemon startup/initialization, query the database for all episodes where `embedding IS NONE`.
2.  If the embedder is present, spawn an asynchronous tokio task to iterate through those episodes, compute their vector embeddings, and update their records.

### 6. HTR & Wisdom Rule Integration
1.  Before the HTR loop triggers LLM ideation, query the `wisdom` table using semantic vector search with the parent hypothesis/context as the query.
2.  Format the top matching rules (e.g. up to 5 rules) as a structured text block (Trigger context, action to avoid, remedy).
3.  Inject this block into the system instructions of the HTR LLM client as active coding rules and constraints.

## Interfaces
We will add new methods to `StorageBackend` trait in [backend.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/db/backend.rs):
```rust
async fn save_wiki_node(&self, node: &WikiNode) -> Result<String>;
async fn relate_nodes(&self, from_id: &str, to_id: &str) -> Result<()>;
async fn get_wiki_node_id_by_vault_path(&self, vault_path: &str) -> Result<Option<String>>;
async fn get_active_scopes(&self) -> Result<Vec<String>>;
async fn delete_by_vault_path(&self, vault_path: &str) -> Result<()>;
async fn search(&self, query: &str, scope: Option<&str>, deep_insight: bool, limit: usize, offset: usize, threshold: f32) -> Result<SearchResponse>;
```

## Data and State

### Struct Definitions (`contracts.rs`)
```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WikiNode {
    pub id: Option<String>,
    pub name: String,
    pub content: String,
    pub scope: String,
    pub vault_path: Option<String>,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    pub total_matches: usize,
    pub has_more: bool,
    pub next_offset: usize,
}
```

### Database Index Schema (`schema.rs`)
```surql
DEFINE INDEX episode_hnsw ON TABLE episode FIELDS embedding HNSW DIMENSION 768 DIST COSINE EFC 100 M 16;
DEFINE INDEX wisdom_hnsw ON TABLE wisdom FIELDS embedding HNSW DIMENSION 768 DIST COSINE EFC 100 M 16;
DEFINE INDEX wiki_node_hnsw ON TABLE wiki_node FIELDS embedding HNSW DIMENSION 768 DIST COSINE EFC 100 M 16;
```

## Error Handling
- **Missing models**: If the Nomis model files are not found, `LocalEmbedder::new()` returns `Err`. The backend logs a warning, sets `embedder = None`, and falls back to substring searches.
- **SurrealDB syntax failures**: Handled via SurrealDB transaction rollback.

## Safety Boundaries
- **Watcher loop prevention**: Watcher uses `WatchIgnoreList` to avoid double-triggers on file updates written programmatically.
- **Worktree Isolation**: Arbor HTR execution remains fully sandboxed.

## Observability
- All changes are logged via `tracing` crate (`info!`, `error!`, `warn!`).
- Database and markdown vault states can be inspected directly via Obsidian or CLI.

## Tradeoffs
- **Rust-side Similarity Computation**: Calculating similarity in Rust using the dot product avoids relying on custom SurrealDB SQL functions which can vary across versions, ensuring robust compatibility.

## Rejected Alternatives
- **Separate Edge Tables**: Creating specific edge tables (e.g. `summarized_in`, `derived_from`) was rejected because it would require rewriting the existing `deep_insight` graph traversal queries. Using `relates_to` for all nodes keeps the query simple and backward-compatible.
