# Tasks

## T1: Add Database Structures and Contracts
- **Purpose**: Upgrade SurrealDB to 3.1.5, define HNSW indexes in the database schema using SurrealDB 3.x syntax, and add the `WikiNode` and `SearchResponse` structs.
- **Related Requirements**: In Scope 1 (HNSW Indexing), 11 (Pagination), 12 (SurrealDB Upgrade)
- **Related Tests**: Test HNSW index definition in SurrealDB
- **Inputs**: [Cargo.toml](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/Cargo.toml), [schema.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/db/schema.rs) and [contracts.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/contracts.rs).
- **Actions**:
  - Update `Cargo.toml` surrealdb dependency version to `3.1.5`.
  - Add/update HNSW vector indexes to `INIT_SCHEMA` in `schema.rs` using SurrealDB 3.x syntax.
  - Add the `WikiNode` and `SearchResponse` struct definitions to `contracts.rs`.
- **Expected Output**: Compiles successfully.
- **Validation**: Schema tests pass.

## T2: Update Storage Backend Trait and Methods
- **Purpose**: Implement `save_wiki_node`, `relate_nodes`, `get_wiki_node_id_by_vault_path`, `get_active_scopes`, and `delete_by_vault_path` in SurrealBackend.
- **Related Requirements**: In Scope 4, 5, 6, 10
- **Related Tests**: `save_wiki_node`, `relate_nodes`, and `delete_by_vault_path` integration tests.
- **Inputs**: [backend.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/db/backend.rs).
- **Actions**:
  - Add methods to the `StorageBackend` trait.
  - Implement these methods on `SurrealBackend`.
- **Expected Output**: Backend compiles successfully.
- **Validation**: Integration tests for new methods pass.

## T3: Implement Embedding Generation, Search Scoring Blending, and Slicing/Pagination
- **Purpose**: Compute wisdom rule embeddings, implement HNSW vector search using `<|100|>` operator with blended scoring/thresholds, and expose pagination metadata.
- **Related Requirements**: In Scope 2, 3, 11
- **Related Tests**: Semantic vector search acceptance tests.
- **Inputs**: [backend.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/db/backend.rs), [api.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/api.rs), and [mcp.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/mcp.rs).
- **Actions**:
  - Update `save_wisdom_rule` to compute embeddings by concatenating target_pattern, action_to_avoid, causal_explanation, and prescribed_remedy.
  - Update `search` and `get_wisdom` in `backend.rs` to fetch 100 HNSW vector candidates using `<|100|>` syntax, compute similarity, filter by threshold, sort by blended score, and slice based on pagination inputs, returning `SearchResponse`.
  - Update REST search handlers in `api.rs` to return `SearchResponse` and accept threshold parameter.
  - Update `mcp.rs` to handle `SearchResponse` and append the pagination notice footer if `has_more` is true.
- **Expected Output**: Semantic vector search is enabled with pagination metadata and footer notifications.
- **Validation**: Vector search queries return pagination metadata, and footer shows up when more results exist.

## T4: Add Wiki Directory Syncing and Deletion Syncing to the Watcher
- **Purpose**: Sync wiki insight/compaction files and propagate file deletions to database record removals.
- **Related Requirements**: In Scope 7, 10
- **Related Tests**: watcher integration tests.
- **Inputs**: [watcher.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/vault/watcher.rs).
- **Actions**:
  - Define `WikiFrontmatter` helper struct.
  - Update `sync_file_to_db` to support path matches containing `"wiki/"`.
  - Update the watcher loop in `start_watching` to check for `is_remove()` events and call `backend.delete_by_vault_path`.
- **Expected Output**: Wiki files are auto-synchronized and deletions are propagated.
- **Validation**: Modifying or removing a file updates the database.

## T5: Update dreaming, Compaction, Scheduler Loops, and HTR/Startup Tasks
- **Purpose**: Relate nodes in the database, implement HTR prompt injection, support dynamic scopes, and trigger asynchronous startup reprocessing.
- **Related Requirements**: In Scope 5, 6, 8, 9
- **Related Tests**: Dynamic scope compaction tests, HTR injection tests, and startup reprocessing tests.
- **Inputs**: [synthesis.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/cognitive/synthesis.rs), [compactor.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/cognitive/compactor.rs), [main.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/main.rs), and [arbor.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/cognitive/arbor.rs).
- **Actions**:
  - Update `synthesis.rs` to save wiki nodes and link them via `db.relate_nodes`.
  - Update `compactor.rs` to save compaction nodes and link them to insights.
  - Update `main.rs` background scheduler loop to fetch active scopes and compact all of them.
  - Update `main.rs` daemon start to run a background tokio task checking for episodes with missing embeddings and computing them.
  - Update HTR coordinator/executor in `arbor.rs` to search for similar wisdom rules and inject them.
- **Expected Output**: Graph edges are created, dynamic scopes compacted, and HTR wisdom integrated.
- **Validation**: Verification check shows relationships, background reprocessing, and HTR injection work.

## T6: Run Verification and Write Walkthrough
- **Purpose**: Validate all changes and verify no regressions.
- **Related Requirements**: Acceptance Criteria
- **Related Tests**: All unit and integration tests.
- **Inputs**: Complete codebase.
- **Actions**:
  - Execute `cargo test`.
  - Verify semantic search manually on a running daemon.
- **Expected Output**: 100% tests pass.
- **Validation**: Validation phase completes successfully.
