# Test Plan - Memory Engine Enhancements

## Unit Tests
- [ ] Test HNSW index definition in SurrealDB 3.1.5.
- [ ] Test dot product similarity computation in Rust.
- [ ] Test score blending formula values: Verify that similarity $S$ is correctly scaled by utility score $U$ according to $S \times (0.7 + 0.3 \times U)$.
- [ ] Test `WikiFrontmatter` parsing from YAML content.

9: ## Integration Tests
10: - [ ] Test `SurrealBackend::save_wiki_node` updates and inserts with name constraints.
11: - [ ] Test `SurrealBackend::save_wisdom_rule` creates rule embeddings using the local ONNX model.
12: - [ ] Test `SurrealBackend::relate_nodes` relates nodes using `relates_to` edge table in SurrealDB without duplicates.
13: - [ ] Test `sync_file_to_db` watcher integration saves `.md` files in `wiki/` to the database.
14: - [ ] Test `SurrealBackend::get_active_scopes` returns distinct scopes.
15: - [ ] Test `SurrealBackend::delete_by_vault_path` deletes records across episode, wisdom, and wiki_node tables.
16: - [ ] Test watcher deletion handler propagates `is_remove` events to database deletions.
17: - [ ] Test startup background reprocessing computes embeddings for null embedding episodes.
18: 
19: ## Acceptance Tests
20: - [ ] Test semantic vector search on episodes returns results matching search query meaning rather than keywords.
21: - [ ] Test deep insight search traverses `relates_to` relations and returns correct related insights and wisdom rules.
22: - [ ] Test compactor scheduler runs compaction across multiple scopes.
23: - [ ] Test HTR ideation retrieves and formats matching Wisdom Rules as system prompt guidelines.

## Edge Cases
- [ ] **No Embeddings present**: Verify search handles episodes or wisdom rules with null `embedding` values gracefully.
- [ ] **Empty scopes**: Compact scope returns immediately with 0 items if scope has no insights.

## Failure Modes
- ONNX model files missing: Verify database falls back to substring contains match without throwing errors.
- SQLite locks: Ensure Cursor/Hermes SQLite db connection is opened in read-only mode during ingestion.

## Regression Tests
- Verify all existing 44 tests pass cleanly.
