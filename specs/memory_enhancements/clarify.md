# Clarify

## Restated Request
Close all identified architectural gaps and implement the three priority areas for Project Mythrax's storage, promotion (dreaming/compaction), and retrieval flows. This includes:
1.  Upgrading the project to the newest SurrealDB version (3.1.5) to ensure access to modern, stable vector search and performance features.
2.  Implementing HNSW vector search in SurrealDB using cosine distance and combining it with utility reinforcement scores for ranking.
3.  Generating vector embeddings for Wisdom Rules (by concatenating target pattern, action to avoid, causal explanation, and prescribed remedy) and Wiki Nodes (by concatenating title and content).
4.  Formulating graph edge connections (`relates_to`) during dreaming and compaction cycles to link episodes, insights, compactions, and wisdom rules.
5.  Removing hardcoded scopes from the compaction scheduler, querying the database dynamically for all active scopes.

## Known Facts
- The local ONNX embedder generates 768-dimensional float vectors using `nomic-embed-text-v1.5.onnx`.
- SurrealDB supports persistent RocksDB storage and HNSW vector indexing.
- SurrealDB 3.1.5 uses standard HNSW index definitions and uses `<|K|>` or `<|K, N|>` vector search query syntax (instead of the legacy or buggy `<|K, DISTANCE_METRIC|>` syntax found in 1.5.6).
- The `deep_insight` search query currently performs a bidirectional graph traversal on the `relates_to` and `mentions` tables:
  `<->(relates_to, mentions)<->(...)`
- File synchronization watcher runs on the Obsidian vault, matching `episodes/` and `wisdom/` directories and performing dual-writes.

## Assumptions
- The default Nomis ONNX model and tokenizer are pre-downloaded and stored in `~/.mythrax/models/`.
- SurrealDB is running locally with full vector and graph query capabilities.
- All relationships between episodes, insights, compactions, and wisdom rules should be stored in the generic `relates_to` edge table.

## Ambiguities
All initial design ambiguities have been resolved:
- **Vector Search Distance Metric**: Cosine similarity is chosen to match the normalized ONNX embeddings.
- **Blending Similarity and Utility**: Blended using a mixed formula: $\text{Final Score} = S \times (0.7 + 0.3 \times U)$ where $S$ is Similarity and $U$ is Utility.
- **Relationship Schema**: The existing `relates_to` edge table will be used for all graph connections.
- **Compaction Scopes**: Scopes will be dynamically resolved from the database.
- **Retrieval Limits & Pagination**: Raise default limits to `15` and introduce threshold filtering ($U \times S > \text{threshold}$, defaults to `0.55`). Include pagination metadata (`has_more`, `total_matches`, `next_offset`) to notify agents of remaining records.
- **SurrealDB Version**: Upgrade dependency from `1.5` to `3.1.5` and replace `MTREE` with the correct `HNSW` index definition.

## Tradeoffs
- **Multiplicative vs. Additive Blending**: Multiplicative blending scales similarity cleanly but can drop highly relevant rules if utility is low. The chosen blended formula $S \times (0.7 + 0.3 \times U)$ guarantees that semantic relevance ($S$) remains the dominant factor, while limiting utility scaling to a $+30\%$ boost or dampening effect.
- **Watcher Sync vs. Explicit Write**: Adding `wiki/` directory support to the watcher ensures that any manual modifications to insights or compactions are automatically indexed. However, bidirectional loop prevention must be handled to avoid write cycles.
- **Rust-Side Slicing**: Rather than executing separate count queries, the backend queries 100 HNSW candidate nodes and filters/slices them in Rust memory. This provides low-latency total counts and pagination metrics.

## Blocking Questions
None. All questions have been answered.
