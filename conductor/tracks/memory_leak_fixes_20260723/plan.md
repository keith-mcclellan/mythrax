# Implementation Plan: Memory Leak Remediation & OOM Crash Prevention

> **Phase ordering rationale:** Memory leaks are fixed before async throughput improvements. Unlocking concurrency while memory bombs exist would accelerate OOM crashes.

## Phase 1: Critical MLX Graph Fixes (FR-1)

Surgical fixes to the primary OOM crash triggers. Safe, isolated, no dependency chain.

- [ ] Task: Add `.eval()` to KV cache concatenation in `Qwen2Attention::forward`
  - [ ] Write test: Verify KV cache tensors are evaluated after concatenation (mock MLX arrays)
  - [ ] Add `k.eval()?` and `v.eval()?` after `concatenate_axis_device` calls in `llm/qwen2_mlx.rs` L182-183
  - [ ] Run tests and confirm pass

- [ ] Task: Add `.eval()` to weight dtype casts in `load_model_weights`
  - [ ] Write test: Verify weight HashMap contains evaluated (non-lazy) arrays after loading
  - [ ] Add `cast_v.eval().unwrap()` after `as_dtype` in `llm/mlx_weights.rs` at both shard path (L224) and single-file path (L239)
  - [ ] Run tests and confirm pass

- [ ] Task: Add joint eval to mxbai cross-encoder logit access
  - [ ] Write test: Verify cross-encoder score function evaluates logits jointly (single forward pass)
  - [ ] Add `mlx_rs::eval(&[&logit_0, &logit_1])?` before `as_slice()` calls in `llm/mxbai_mlx.rs` L446-449
  - [ ] Add `mlx_rs::eval(&[&logit_0, &logit_1])?` before `as_slice()` calls in `llm/mxbai_mlx.rs` L492-493
  - [ ] Run tests and confirm pass

- [ ] Task: Migrate embedding cache from binary to SQLite-only
  - [ ] Write test: Verify `flush_dirty` with SQLite path correctly persists and retrieves dirty entries without loading entire cache
  - [ ] Write test: Verify cache capacity is enforced during flush
  - [ ] Remove binary `flush_dirty` code path (embeddings.rs L303-375)
  - [ ] Update `flush_dirty_default()` to always use SQLite path
  - [ ] Add migration: convert existing `embedding_cache.bin` to SQLite on first run
  - [ ] Run tests and confirm pass

- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 2: Search Pipeline Memory Safety (FR-2)

The TF-IDF cache-miss bomb is the largest single memory allocation in the codebase. Must be fixed before any concurrency improvements.

- [ ] Task: Build incremental IDF index for BM25/FTS
  - [ ] Write test: Verify IDF term counts are updated incrementally on episode insert without loading all content
  - [ ] Write test: Verify FTS search uses pre-computed IDF index on cache miss (no `SELECT VALUE content FROM episode`)
  - [ ] Create `idf_index` table in SurrealDB schema (term → document_frequency, total_docs)
  - [ ] Add `update_idf_index(episode_id)` function that incrementally updates term counts on episode save
  - [ ] Wire `update_idf_index` into `save_episode` and `save_episodes_batch` code paths
  - [ ] Refactor `search_pipeline.rs` L1927 to read from `idf_index` table instead of loading all episode content
  - [ ] Run tests and confirm pass

- [ ] Task: Add LIMIT constraints to temporal neighbor graph traversals
  - [ ] Write test: Verify temporal expansion returns at most N results per hop level
  - [ ] Write test: Verify depth-3 traversal on dense graph does not exceed memory bounds
  - [ ] Add `LIMIT 50` to each hop level in `search_pipeline.rs` L2323-2346
  - [ ] Add `LIMIT 50` to the secondary temporal expansion query at L2341-2346
  - [ ] Run tests and confirm pass

- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 3: Complete Pagination Migration (FR-3, FR-8)

Exhaustive sweep of ALL unpaginated bulk-load call sites.

- [ ] Task: Implement paginated query variants in CRUD layer
  - [ ] Write test: Verify `get_episodes_paginated(limit, offset)` returns correct subset with proper offset/limit
  - [ ] Write test: Verify `get_wiki_nodes_paginated(limit, offset)` returns correct subset
  - [ ] Write test: Verify `get_wisdom_rules_paginated(limit, offset)` returns correct subset
  - [ ] Write test: Verify `get_episodes_by_node_type_paginated(type, limit, offset)` returns correct subset
  - [ ] Add paginated variants to `crud_operations.rs` and `backend.rs` trait
  - [ ] Run tests and confirm pass

- [ ] Task: Migrate ALL `get_all_episodes()` callers (12 sites)
  - [ ] Refactor `compactor.rs` L252, L1211
  - [ ] Refactor `synthesis.rs` L891, L917, L2987
  - [ ] Refactor `precompact.rs` L127
  - [ ] Refactor `vault_handlers.rs` L41, L72
  - [ ] Refactor `manage_handlers.rs` L338, L1214
  - [ ] Refactor `vault/ingestion.rs` L606
  - [ ] Refactor `blackboard.rs` L189
  - [ ] Run tests and confirm pass

- [ ] Task: Migrate ALL `get_all_wiki_nodes()` callers (7 sites)
  - [ ] Refactor `synthesis.rs` L504, L1949, L2440, L3199
  - [ ] Refactor `harvest.rs` L297
  - [ ] Refactor `meta_skill.rs` L127
  - [ ] Run tests and confirm pass

- [ ] Task: Migrate ALL `get_all_wisdom_rules()` callers
  - [ ] Refactor `harvest.rs` L342
  - [ ] Refactor `synthesis.rs` L2440 (wisdom deduplication)
  - [ ] Run tests and confirm pass

- [ ] Task: Migrate `get_episodes_by_node_type()` callers
  - [ ] Refactor `compactor.rs` L180
  - [ ] Refactor `synthesis.rs` L1967
  - [ ] Run tests and confirm pass

- [ ] Task: Migrate `get_all_registered_transcripts()` callers
  - [ ] Refactor `synthesis.rs` L781
  - [ ] Run tests and confirm pass

- [ ] Task: Migrate `get_all_episodes()` / `meta_skill.rs` L126 callers
  - [ ] Refactor `meta_skill.rs` L126
  - [ ] Run tests and confirm pass

- [ ] Task: Paginate startup missing-embedding backfill
  - [ ] Write test: Verify startup backfill processes records in bounded batches
  - [ ] Refactor daemon.rs L126, L152, L178 to use `LIMIT`/`OFFSET` loops
  - [ ] Run tests and confirm pass

- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 4: Streaming-to-Disk Cognitive Pipeline (FR-4)

Convert the cognitive pipeline from in-memory accumulation to incremental vault writes. Each stage writes its output to md files as produced, drops the in-memory data, then the next stage reads from disk.

- [ ] Task: Stream episode summaries to vault during dreaming
  - [ ] Write test: Verify episode summaries are flushed to `wiki/<scope>/episodes/*.md` immediately after LLM generation, not held in batch Vec
  - [ ] Refactor `synthesis.rs` dreaming loop (L869-883): process unprocessed episodes in bounded chunks, write each summary to vault md file before processing next chunk
  - [ ] Drop `all_episodes` cache (L891) — replace with paginated DB queries for centroid calculation using running mean
  - [ ] Run tests and confirm pass

- [ ] Task: Stream DBSCAN cluster assignments to disk manifest
  - [ ] Write test: Verify cluster assignments are serialized to `wiki/<scope>/.clusters/<timestamp>.json` and individual members read back during synthesis
  - [ ] Refactor `synthesis.rs` DBSCAN flow (L1256-1258): write cluster manifest to vault, then read cluster members back one-at-a-time during insight synthesis
  - [ ] Refactor `compactor.rs` hierarchical DBSCAN (L848-856): serialize cluster map to temp json, process clusters sequentially from disk
  - [ ] Run tests and confirm pass

- [ ] Task: Stream insight synthesis to vault incrementally
  - [ ] Write test: Verify each synthesized insight is written to `wiki/<scope>/insights/*.md` immediately and dropped from memory before next cluster
  - [ ] Write test: Verify no more than 50 insights are held in memory simultaneously
  - [ ] Refactor cluster-to-insight synthesis loop in `synthesis.rs` (L1258-1400): write insight md, drop from memory, proceed to next cluster
  - [ ] Refactor scope insights loading (L977-978, L1505): use paginated vault reads instead of `load_insights()` loading all files
  - [ ] Run tests and confirm pass

- [ ] Task: Stream direction promotion to vault incrementally
  - [ ] Write test: Verify direction nodes are written to `wiki/<scope>/directions/*.md` immediately after promotion evaluation
  - [ ] Refactor direction backpropagation (synthesis.rs L2988-3002): load directions paginated, process one-at-a-time, write result to vault, drop, next
  - [ ] Refactor direction promotion drift metrics (L3060-3155): evaluate and write one candidate at a time
  - [ ] Run tests and confirm pass

- [ ] Task: Stream wisdom graduation to vault incrementally
  - [ ] Write test: Verify graduated wisdom rules are written to `wisdom/*.md` immediately after cross-scope matching
  - [ ] Refactor graduation candidates (synthesis.rs L1946): load candidates paginated instead of `get_all_wiki_nodes()`
  - [ ] Refactor graduation clusters (L2168-2182): process one cluster at a time, write wisdom rule to vault, drop, next
  - [ ] Refactor wisdom deduplication (L2440): stream existing rules from DB paginated for comparison
  - [ ] Refactor direction-to-wisdom graduation (L3203-3246): process one direction pair at a time
  - [ ] Run tests and confirm pass

- [ ] Task: Stream pruned/archived nodes to vault immediately
  - [ ] Write test: Verify pruned nodes are moved to `vault/archive/` and dropped from memory immediately, not batched
  - [ ] Refactor GC candidates (compactor.rs L140): process one node at a time — archive file, delete from DB, drop reference
  - [ ] Refactor procedural episode trimming (compactor.rs L180): paginate active procs, archive excess one-at-a-time
  - [ ] Refactor near-duplicate merging (compactor.rs L253): compare pairs using paginated iteration, merge+archive immediately per pair
  - [ ] Refactor decayed episode archival (compactor.rs L1195-1211): paginate episodes, compute decay per batch, archive immediately
  - [ ] Run tests and confirm pass

- [ ] Task: Stream compaction summaries one cluster at a time
  - [ ] Write test: Verify compaction loop writes each cluster summary to `wiki/<scope>/compactions/*.md` and releases prompt buffer before next cluster
  - [ ] Write test: Verify outlier insights are written individually, not accumulated in batch Vec
  - [ ] Refactor compaction loop (compactor.rs L861-1013): flush prompt_content and LLM response after each cluster write
  - [ ] Refactor outlier handling (compactor.rs L1018-1090): write each outlier insight individually
  - [ ] Run tests and confirm pass

- [ ] Task: Cap synthesis cluster prompt concatenation
  - [ ] Write test: Verify `insights_with_scope_labels` string is truncated at 32K token budget
  - [ ] Add token-budget truncation to synthesis.rs L2204-2210
  - [ ] Run tests and confirm pass

- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 5: Async Runtime Safety (FR-5, FR-7)

Now safe to unlock concurrency — all memory bombs are fixed.

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

## Phase 6: Proportional Growth Mitigations (FR-9)

Address remaining medium-severity memory scaling issues.

- [ ] Task: Add sliding window to transcript mining tool sequence
  - [ ] Write test: Verify `mine_transcript` tool_sequence Vec does not exceed window size
  - [ ] Implement sliding window or periodic flush for `tool_sequence` in `precompact.rs` L125-160
  - [ ] Run tests and confirm pass

- [ ] Task: Add payload size limits to API batch endpoint
  - [ ] Write test: Verify `save_episodes_batch_handler` rejects payloads exceeding limit
  - [ ] Add size check to api.rs L120
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
