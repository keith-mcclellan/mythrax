# Implementation Plan: Memory Leak Remediation & OOM Crash Prevention

> **Phase ordering rationale:** Memory leaks are fixed before async throughput improvements. Unlocking concurrency while memory bombs exist would accelerate OOM crashes.
>
> **Phase gate protocol:** Every phase ends with the **complete 14-step Phase Completion Protocol** defined in `conductor/workflow.md` (unit tests → dev50 → manual verification → user confirmation → conductor-review → adversarial CTO review → conditional commit → git notes → checkpoint SHA). No phase may bypass any step.

## Phase 1: Critical MLX Graph Fixes (FR-1, FR-6)

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

- [ ] Task: Execute Phase Completion Protocol (workflow.md Steps 1-14)

## Phase 2: Search Pipeline Memory Safety (FR-2)

The TF-IDF cache-miss bomb is the largest single memory allocation in the codebase. Must be fixed before any concurrency improvements.

- [ ] Task: Create `idf_index` table and initialization logic
  - [ ] Write test: Verify `idf_index` table is created during daemon startup INIT_SCHEMA step
  - [ ] Add `idf_index` table definition (term: String, document_frequency: i64, total_docs: i64, scope: String) to `src/db/surreal_init.rs` INIT_SCHEMA
  - [ ] Run tests and confirm pass

- [ ] Task: Build incremental IDF update function
  - [ ] Write test: Verify `update_idf_index(episode_id)` correctly increments term document frequencies on episode insert
  - [ ] Write test: Verify `update_idf_index` correctly decrements term document frequencies on episode delete
  - [ ] Add `update_idf_index` to `crud_operations.rs`
  - [ ] Wire `update_idf_index` into `save_episode` and `save_episodes_batch` code paths in `backend.rs`
  - [ ] Run tests and confirm pass

- [ ] Task: Replace TF-IDF cache-miss bulk load with IDF index lookup
  - [ ] Write test: Verify FTS search uses pre-computed IDF index on cache miss (no `SELECT VALUE content FROM episode`)
  - [ ] Refactor `search_pipeline.rs` L1927 to read from `idf_index` table instead of loading all episode content
  - [ ] Run tests and confirm pass

- [ ] Task: Add LIMIT constraints to temporal neighbor graph traversals
  - [ ] Write test: Verify temporal expansion returns at most 50 results per hop level
  - [ ] Write test: Verify depth-3 traversal on dense graph does not exceed memory bounds
  - [ ] Add `LIMIT 50` to each hop level in `search_pipeline.rs` L2323-2328 (preds_1/2/3, succs_1/2/3)
  - [ ] Add `LIMIT 50` to the secondary temporal expansion query at L2341-2346
  - [ ] Run tests and confirm pass

- [ ] Task: Execute Phase Completion Protocol (workflow.md Steps 1-14)

## Phase 3: Complete Pagination Migration (FR-3, FR-8)

Exhaustive sweep of ALL unpaginated bulk-load call sites.

- [ ] Task: Implement paginated query variants in CRUD layer
  - [ ] Write test: Verify `get_episodes_paginated(limit, offset)` returns correct subset with proper offset/limit
  - [ ] Write test: Verify `get_wiki_nodes_paginated(limit, offset)` returns correct subset
  - [ ] Write test: Verify `get_wisdom_rules_paginated(limit, offset)` returns correct subset
  - [ ] Write test: Verify `get_episodes_by_node_type_paginated(type, limit, offset)` returns correct subset
  - [ ] Write test: Verify `get_registered_transcripts_paginated(limit, offset)` returns correct subset
  - [ ] Add paginated variants to `crud_operations.rs` and `backend.rs` trait
  - [ ] Run tests and confirm pass

- [ ] Task: Implement hash-based deduplication for wisdom rules and episodes
  - [ ] Write test: Verify `find_duplicate_by_content_hash(hash)` returns matching record without full table scan
  - [ ] Write test: Verify `content_hash` is computed and stored on episode/wisdom save
  - [ ] Add `content_hash` field (SHA-256 of normalized content) to episode and wisdom_rule records
  - [ ] Add `find_duplicate_by_content_hash` query to `crud_operations.rs` using DB index lookup
  - [ ] Refactor wisdom deduplication in `synthesis.rs` L2440 to use hash lookup instead of paginated comparison
  - [ ] Refactor near-duplicate detection in `compactor.rs` L253 to use hash pre-filter before embedding comparison
  - [ ] Run tests and confirm pass

- [ ] Task: Migrate `get_all_episodes()` callers — cognitive pipeline (3 files)
  - [ ] Refactor `compactor.rs` L252, L1211
  - [ ] Refactor `synthesis.rs` L891, L917, L2987
  - [ ] Refactor `precompact.rs` L127
  - [ ] Run tests and confirm pass

- [ ] Task: Migrate `get_all_episodes()` callers — HTTP handlers (2 files)
  - [ ] Refactor `vault_handlers.rs` L41, L72
  - [ ] Refactor `manage_handlers.rs` L338, L1214
  - [ ] Run tests and confirm pass

- [ ] Task: Migrate `get_all_episodes()` callers — internal pipelines (2 files)
  - [ ] Refactor `vault/ingestion.rs` L606
  - [ ] Refactor `blackboard.rs` L189
  - [ ] Run tests and confirm pass

- [ ] Task: Migrate ALL `get_all_wiki_nodes()` callers (3 files)
  - [ ] Refactor `synthesis.rs` L504, L1949, L2440, L3199
  - [ ] Refactor `harvest.rs` L297
  - [ ] Refactor `meta_skill.rs` L127
  - [ ] Run tests and confirm pass

- [ ] Task: Migrate ALL `get_all_wisdom_rules()` callers (2 files)
  - [ ] Refactor `harvest.rs` L342
  - [ ] Refactor `synthesis.rs` L2440 (wisdom deduplication — now uses hash-based, but paginated fallback)
  - [ ] Run tests and confirm pass

- [ ] Task: Migrate `get_episodes_by_node_type()` callers (2 files)
  - [ ] Refactor `compactor.rs` L180
  - [ ] Refactor `synthesis.rs` L1967
  - [ ] Run tests and confirm pass

- [ ] Task: Migrate `get_all_registered_transcripts()` and `get_all_episodes()` in meta_skill (2 files)
  - [ ] Refactor `synthesis.rs` L781
  - [ ] Refactor `meta_skill.rs` L126
  - [ ] Run tests and confirm pass

- [ ] Task: Paginate startup missing-embedding backfill
  - [ ] Write test: Verify startup backfill processes records in bounded batches
  - [ ] Refactor daemon.rs L126, L152, L178 to use `LIMIT`/`OFFSET` loops
  - [ ] Run tests and confirm pass

- [ ] Task: Execute Phase Completion Protocol (workflow.md Steps 1-14)

## Phase 4: Streaming-to-Disk Cognitive Pipeline (FR-4)

Convert the cognitive pipeline from in-memory accumulation to incremental vault writes. Each stage writes its output to vault markdown files (for human-readable artifacts) or SurrealDB temporary records (for machine-only intermediate state) as produced, drops the in-memory data, then the next stage reads from the persisted store.

### Storage Routing

| Artifact | Destination | Rationale |
|----------|-------------|-----------|
| Episode summaries | Vault md (`wiki/<scope>/episodes/*.md`) | Human-readable, version-controlled |
| DBSCAN cluster assignments | SurrealDB `pipeline_cluster` table | Ephemeral machine state, needs transaction safety, avoids FS watcher re-ingestion |
| Synthesized insights | Vault md (`wiki/<scope>/insights/*.md`) | Human-readable, version-controlled |
| Direction promotions | Vault md (`wiki/<scope>/directions/*.md`) | Human-readable, version-controlled |
| Wisdom rules | Vault md (`wisdom/*.md`) + DB `wisdom_rule` | Human-readable + queryable |
| Pruned/archived nodes | Vault `archive/` | Human-readable audit trail |
| Compaction summaries | Vault md (`wiki/<scope>/compactions/*.md`) | Human-readable, version-controlled |

- [ ] Task: Create `pipeline_cluster` temporary table for DBSCAN state
  - [ ] Write test: Verify `pipeline_cluster` records can be inserted, queried by run_id, and bulk-deleted after synthesis
  - [ ] Add `pipeline_cluster` table definition to INIT_SCHEMA (fields: run_id, cluster_id, episode_id, scope, created_at)
  - [ ] Add `save_cluster_assignment`, `get_cluster_members(run_id, cluster_id)`, and `delete_pipeline_run(run_id)` to CRUD layer
  - [ ] Run tests and confirm pass

- [ ] Task: Stream episode summaries to vault during dreaming
  - [ ] Write test: Verify episode summaries are flushed to `wiki/<scope>/episodes/*.md` immediately after LLM generation, not held in batch Vec
  - [ ] Refactor `synthesis.rs` dreaming loop (L869-883): process unprocessed episodes in bounded chunks, write each summary to vault md file before processing next chunk
  - [ ] Drop `all_episodes` cache (L891) — replace with paginated DB queries for centroid calculation using running mean
  - [ ] Run tests and confirm pass

- [ ] Task: Stream DBSCAN cluster assignments to SurrealDB
  - [ ] Write test: Verify cluster assignments are stored in `pipeline_cluster` table with unique run_id and individual members retrieved during synthesis
  - [ ] Write test: Verify `pipeline_cluster` records are deleted after successful synthesis completion
  - [ ] Refactor `synthesis.rs` DBSCAN flow (L1256-1258): write cluster assignments to `pipeline_cluster` table, then query members per-cluster during insight synthesis
  - [ ] Refactor `compactor.rs` hierarchical DBSCAN (L848-856): write cluster assignments to `pipeline_cluster` table, process clusters sequentially by querying from DB
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
  - [ ] Refactor wisdom deduplication (L2440): use hash-based lookup (Phase 3) for comparison
  - [ ] Refactor direction-to-wisdom graduation (L3203-3246): process one direction pair at a time
  - [ ] Run tests and confirm pass

- [ ] Task: Stream pruned nodes — GC and procedural trimming
  - [ ] Write test: Verify pruned nodes are moved to `vault/archive/` and dropped from memory immediately, not batched
  - [ ] Refactor GC candidates (compactor.rs L140): process one node at a time — archive file, delete from DB, drop reference
  - [ ] Refactor procedural episode trimming (compactor.rs L180): paginate active procs, archive excess one-at-a-time
  - [ ] Run tests and confirm pass

- [ ] Task: Stream pruned nodes — near-duplicate merging and decay archival
  - [ ] Write test: Verify near-duplicate merging archives each pair immediately, not in batch
  - [ ] Write test: Verify decayed episodes are archived and dropped from memory per-batch
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

- [ ] Task: Execute Phase Completion Protocol (workflow.md Steps 1-14)

## Phase 5: Async Runtime Safety (FR-5, FR-7)

Now safe to unlock concurrency — all memory bombs are fixed.

- [ ] Task: Identify and categorize all callers of blocking embed functions
  - [ ] Audit all callers of `embed()`, `embed_batch()`, `embed_sub_batch()` across the codebase
  - [ ] Categorize each caller as: (a) already in async context, (b) in sync context requiring `block_on` bridge, or (c) in trait impl requiring signature change
  - [ ] Document the migration plan for each caller in a scratch note before proceeding
  - [ ] Run tests and confirm pass (no code changes, audit only)

- [ ] Task: Replace blocking semaphore spin-loops with async semaphores
  - [ ] Write test: Verify embedding semaphore acquisition is non-blocking and yields to tokio runtime
  - [ ] Replace `std::thread::sleep` + `try_acquire()` with `tokio::sync::Semaphore::acquire().await` in:
    - `embeddings.rs` L839-843
    - `embeddings.rs` L992-996
    - `llm/mxbai_mlx.rs` L405
  - [ ] Run tests and confirm pass

- [ ] Task: Convert embed functions to async and update async-context callers
  - [ ] Convert `embed()`, `embed_batch()`, `embed_sub_batch()` to async functions
  - [ ] Update all callers identified in category (a) — those already in async context
  - [ ] Run tests and confirm pass

- [ ] Task: Bridge sync-context callers to async embed functions
  - [ ] For callers identified in category (b): add `tokio::runtime::Handle::current().block_on()` bridge
  - [ ] For callers identified in category (c): update trait signatures if needed, or add async trait variants
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

- [ ] Task: Execute Phase Completion Protocol (workflow.md Steps 1-14)

## Phase 6: Proportional Growth Mitigations (FR-9)

Address remaining medium-severity memory scaling issues.

- [ ] Task: Add sliding window to transcript mining tool sequence
  - [ ] Write test: Verify `mine_transcript` tool_sequence Vec does not exceed window size
  - [ ] Implement sliding window or periodic flush for `tool_sequence` in `precompact.rs` L125-160
  - [ ] Run tests and confirm pass

- [ ] Task: Stream chunk processing in Forge pipeline
  - [ ] Write test: Verify forge pipeline processes document chunks in bounded batches, not loading entire document into memory
  - [ ] Refactor `forge.rs` to use streaming chunk iteration with bounded buffer (identify specific accumulation points via `Vec<Chunk>` patterns)
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

- [ ] Task: Execute Phase Completion Protocol (workflow.md Steps 1-14)

## Phase 7: Architecture & Data Flow Documentation Update

Update project documentation to reflect all architectural changes made in Phases 1-6.

- [ ] Task: Update ARCHITECTURE.md
  - [ ] Update Section 2 (Dual-Engine Storage): document SQLite embedding cache migration, incremental IDF indexer, `pipeline_cluster` temporary table
  - [ ] Update Section 3 (Three-Tiered Model Broker): document MLX `.eval()` requirements for KV caches, weight casts, and cross-encoder logits
  - [ ] Update Section 4 (Cognitive Scheduling): document streaming-to-disk pipeline architecture (vault md for human-readable, SurrealDB for machine state), bounded pagination for all DB queries, temporal traversal LIMIT constraints
  - [ ] Update Section 5 (Graceful Shutdown): document CancellationToken lifecycle for background tasks, async semaphore model
  - [ ] Update Section 6 (End-to-End Data Flow): update data flow diagram to reflect streaming pipeline, IDF indexer, hash-based deduplication, and bounded graph traversals
  - [ ] Add new Section: Memory Safety Invariants (pipeline stage memory cap ≤50 items, no `get_all_*` unbounded queries, all MLX operations evaluated before caching, hash-based deduplication for content comparison)

- [ ] Task: Update inline code documentation
  - [ ] Add doc comments to all new paginated query functions in `crud_operations.rs` and `backend.rs`
  - [ ] Add doc comments to `update_idf_index`, `pipeline_cluster` CRUD functions, and streaming pipeline functions
  - [ ] Add safety comments at all `.eval()` call sites explaining the lazy graph accumulation risk

- [ ] Task: Execute Phase Completion Protocol (workflow.md Steps 1-14)
