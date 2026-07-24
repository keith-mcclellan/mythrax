# Implementation Plan: Memory Leak Remediation & OOM Crash Prevention

## Phase 1: Critical MLX Graph Fixes (FR-1, FR-4)

These are the primary OOM crash triggers. Surgical fixes with immediate impact.

- [ ] Task: Add `.eval()` to KV cache concatenation in `Qwen2Attention::forward`
  - [ ] Write test: Verify KV cache tensors are evaluated after concatenation (mock MLX arrays)
  - [ ] Add `k.eval()?` and `v.eval()?` after `concatenate_axis_device` calls in `llm/qwen2_mlx.rs` L182-183
  - [ ] Run tests and confirm pass

- [ ] Task: Add `.eval()` to weight dtype casts in `load_model_weights`
  - [ ] Write test: Verify weight HashMap contains evaluated (non-lazy) arrays after loading
  - [ ] Add `cast_v.eval().unwrap()` after `as_dtype` in `llm/mlx_weights.rs` at both shard path (L224) and single-file path (L239)
  - [ ] Run tests and confirm pass

- [ ] Task: Migrate embedding cache from binary to SQLite-only
  - [ ] Write test: Verify `flush_dirty` with SQLite path correctly persists and retrieves dirty entries without loading entire cache
  - [ ] Write test: Verify cache capacity is enforced during flush
  - [ ] Remove binary `flush_dirty` code path (embeddings.rs L303-375)
  - [ ] Update `flush_dirty_default()` to always use SQLite path
  - [ ] Add migration: convert existing `embedding_cache.bin` to SQLite on first run
  - [ ] Run tests and confirm pass

- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 2: Async Runtime Safety (FR-3, FR-5)

Eliminate deadlock risk and enable clean daemon lifecycle.

- [ ] Task: Replace blocking semaphore spin-loops with async semaphores
  - [ ] Write test: Verify embedding semaphore acquisition is non-blocking and yields to tokio runtime
  - [ ] Replace `std::thread::sleep` + `try_acquire()` with `tokio::sync::Semaphore::acquire().await` in:
    - `embeddings.rs` L839-843
    - `embeddings.rs` L992-996
    - `llm/mxbai_mlx.rs` L405
  - [ ] Convert `embed()`, `embed_batch()`, `embed_sub_batch()` to async functions
  - [ ] Update all callers of these functions
  - [ ] Run tests and confirm pass

- [ ] Task: Replace blocking sleeps in daemon.rs
  - [ ] Write test: Verify daemon startup/retry loops use async sleep
  - [ ] Replace `std::thread::sleep` at daemon.rs L631 and L638 with `tokio::time::sleep`
  - [ ] Run tests and confirm pass

- [ ] Task: Add CancellationToken to all daemon background tasks
  - [ ] Write test: Verify all background tasks terminate within 5 seconds when cancellation is signaled
  - [ ] Create shared `CancellationToken` in daemon startup
  - [ ] Pass token to all 5 `tokio::spawn` loops (daemon.rs L228, L238, L249, L261, L268)
  - [ ] Replace bare `loop {}` with `loop { tokio::select! { _ = token.cancelled() => break, ... } }`
  - [ ] Wire token cancellation into the graceful shutdown sequence
  - [ ] Run tests and confirm pass

- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 3: Database Query Pagination (FR-2, FR-6)

Eliminate unbounded bulk data loads.

- [ ] Task: Implement paginated `get_episodes_paginated(limit, offset)` in CRUD layer
  - [ ] Write test: Verify paginated query returns correct subset with proper offset/limit
  - [ ] Write test: Verify paginated iteration covers all records when iterated to exhaustion
  - [ ] Add `get_episodes_paginated` to `crud_operations.rs` and `backend.rs` trait
  - [ ] Run tests and confirm pass

- [ ] Task: Implement paginated variants for wiki_nodes and wisdom_rules
  - [ ] Write test: Verify `get_wiki_nodes_paginated` and `get_wisdom_rules_paginated` return correct subsets
  - [ ] Add paginated queries to CRUD layer and backend trait
  - [ ] Run tests and confirm pass

- [ ] Task: Migrate compactor callers to paginated queries
  - [ ] Write test: Verify compactor processes episodes in bounded batches
  - [ ] Refactor `compactor.rs` L252 and L1211 to use paginated iteration
  - [ ] Refactor `synthesis.rs` L504 (wiki_nodes) and L891 (episodes) to use paginated iteration
  - [ ] Run tests and confirm pass

- [ ] Task: Migrate precompact, vault, and MCP handler callers
  - [ ] Write test: Verify precompact hook processes episodes in bounded batches
  - [ ] Refactor `precompact.rs` L127 to use paginated query
  - [ ] Refactor `vault_handlers.rs` L41 and L72 to use paginated query
  - [ ] Refactor `manage_handlers.rs` L1214 to use paginated query
  - [ ] Refactor `vault/ingestion.rs` L606 to use paginated query
  - [ ] Run tests and confirm pass

- [ ] Task: Paginate startup missing-embedding backfill
  - [ ] Write test: Verify startup backfill processes records in bounded batches
  - [ ] Refactor daemon.rs L126, L152, L178 to use `LIMIT`/`OFFSET` loops
  - [ ] Run tests and confirm pass

- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 4: Proportional Growth Mitigations (FR-7)

Address medium-severity memory scaling issues.

- [ ] Task: Add sliding window to transcript mining tool sequence
  - [ ] Write test: Verify `mine_transcript` tool_sequence Vec does not exceed window size
  - [ ] Implement sliding window or periodic flush for `tool_sequence` in `precompact.rs` L125-160
  - [ ] Run tests and confirm pass

- [ ] Task: Add payload size limits to API batch endpoint
  - [ ] Write test: Verify `save_episodes_batch_handler` rejects payloads exceeding limit
  - [ ] Add `axum::extract::ContentLengthLimit` or manual size check to api.rs L120
  - [ ] Run tests and confirm pass

- [ ] Task: Switch vault bulk ingestion to batch inserts
  - [ ] Write test: Verify `bulk_ingest_vault` uses `save_episodes_batch` for chunk inserts
  - [ ] Refactor `vault/ingestion.rs` L1021 to accumulate chunks and use `save_episodes_batch`
  - [ ] Run tests and confirm pass

- [ ] Task: Fix VRAM tracking state desync
  - [ ] Write test: Verify `acquire_llm` updates `active_tier` and `last_weak_ref` on cache hit
  - [ ] Update early-return path in `llm/mod.rs` L1528 to set `active_tier` and `last_weak_ref`
  - [ ] Run tests and confirm pass

- [ ] Task: Bound completions proxy chat history concatenation
  - [ ] Write test: Verify prompt string is bounded by max token limit
  - [ ] Add truncation logic to `completions_proxy_handler` in api.rs L613-622
  - [ ] Run tests and confirm pass

- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)
