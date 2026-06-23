# Requirements - Memory Engine Enhancements

## Problem
Although Project Mythrax generates vector embeddings locally for raw episodes, the system does not utilize these embeddings for retrieval. All memory searches (`/v1/search` and `search_memories` MCP) fall back to substring-contains matching. 

Additionally, higher-level abstract layers (Wisdom Rules, Wiki Insights, Compactions) are stored without embeddings, and there are no relational graph edges connecting them in SurrealDB. As a result, agents cannot execute semantic searches or perform graph traversal queries to retrieve relevant rules, insights, or compactions. Finally, the compactor scheduler is restricted to the `"general"` scope.

## Outcome
A fully semantic and graph-linked memory engine where:
1.  All memory searches query SurrealDB using local vector similarity via an HNSW index, combined with reinforcement learning utility scores.
2.  Wisdom Rules and Wiki Nodes (Insights, Compactions, Global Synthesis) have vector embeddings computed on creation and update.
3.  Bidirectional graph relationships are created between related episodes, insights, compactions, and rules in SurrealDB.
4.  Compactions are automatically run for all active scopes in the database.

## User Value
- Agents retrieve highly relevant historical memories using semantic meaning rather than exact keywords.
- Agents retrieve related rules and compactions via deep insight graph searches.
- Wisdom Rules are reinforced by agent feedback and prioritized by utility.
- All namespaces/scopes are compacted automatically without configuration.

## In Scope
1.  **HNSW Indexing**: Update the SurrealDB schema to define HNSW indexes for `episode`, `wisdom`, `wiki_node`, `entity`, and `handoff` tables using SurrealDB 3.x syntax.
2.  **Vector Retrieval**: Update backend searches for episodes and wisdom to use HNSW cosine similarity.
3.  **Utility Blending**: Rank search results using the blended formula: $\text{Final Score} = S \times (0.7 + 0.3 \times U)$.
4.  **Embeddings for Rules & Wiki Nodes**: Compute embeddings for Wisdom Rules (concatenated fields) and Wiki Nodes (name + content).
5.  **Graph Relationships**: Relate episodes, insights, compactions, and rules in SurrealDB using the `relates_to` edge table during dreaming and compaction.
6.  **Dynamic Scopes**: Query active scopes in the database and run compaction for all of them.
7.  **File Watcher Sync**: Sync `.md` files in `wiki/` to the `wiki_node` table in SurrealDB, with loop prevention.
8.  **Automatic Embedding Reprocessing**: Automatically detect episodes with missing embeddings on backend initialization, and trigger an asynchronous background reprocessing run.
9.  **HTR & Wisdom Integration**: Retrieve semantically similar Wisdom Rules and inject them as system guidelines into the LLM ideator prompt before HTR ideation.
10. **File Deletion Watcher Sync**: Handle file removal events (`is_remove`) in the watcher and delete the corresponding database records.
11. **Pagination & Threshold Filtering**: Raise default limits to `15`, support custom thresholds (default `0.55`), and return pagination metadata (`has_more`, `total_matches`, `next_offset`). Format a metadata notice in the MCP response to notify agents when more results exist.
12. **SurrealDB Upgrade**: Upgrade database dependency to `3.1.5` and implement stable index and query syntax.

## Out of Scope
- Defining custom edge tables other than `relates_to`.
- Changing the reinforcement learning feedback algorithm itself.

## Inputs
- `query` text from search.
- Obsidian Vault files in `episodes/`, `wisdom/`, and `wiki/`.
- SurrealDB persistent database tables.
- Local ONNX embedder model files.

## Outputs
- Search results with similarity scores, utility ratings, and related nodes.
- Pagination metadata (`total_matches`, `has_more`, `next_offset`).
- Updated SurrealDB records with computed embeddings.
- Graph relation edges (`relates_to` table) in SurrealDB.

## Constraints
- **Graceful Fallback**: If the ONNX embedder model is missing or fails, search must fall back to substring-contains matching.
- **Loop Prevention**: Watcher sync must ignore files for 2 seconds when updated programmatically to prevent infinite loops.

## Assumptions
- Embeddings generated are always 768-dimensional.
- The model `nomic-embed-text-v1.5.onnx` generates normalized vectors.

## Risks and Edge Cases
1.  **Missing Embeddings in DB**: Historical records might have `None` embeddings.
    - *Mitigation*: In vector search query, filter or handle null embeddings gracefully (fall back to substring search if query embedding fails).
2.  **SurrealDB compatibility**: Different syntax versions for HNSW vector index.
    - *Mitigation*: Use verified SurrealDB 3.x HNSW index definition and KNN query operator (`<|K|>`).

## Acceptance Criteria
- [ ] SurrealDB package version is upgraded to `3.1.5` in Cargo.toml.
- [ ] Schema applies HNSW indexes successfully.
- [ ] `save_wisdom_rule` writes rule embeddings to SurrealDB.
- [ ] Watcher synchronizes `wiki/` files and saves them to the database as `wiki_node` records with embeddings.
- [ ] Search query executes vector search using the `<|K|>` operator.
- [ ] Search results are ordered by blended score $S \times (0.7 + 0.3 \times U)$.
- [ ] Relationships are created in SurrealDB connecting episodes to insights, insights to compactions, and rules to episodes.
- [ ] The compaction scheduler compacts all active scopes.
- [ ] Backend automatically triggers background reprocessing on startup for episodes with missing embeddings.
- [ ] HTR ideation is injected with semantically matched Wisdom Rules.
- [ ] Deleting a markdown file from the vault removes its corresponding database record.
- [ ] Search results return pagination metadata, and MCP server outputs notification footers when `has_more` is true.
- [ ] All unit and integration tests pass.
