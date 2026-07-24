# Specification: Memory Leak Remediation & OOM Crash Prevention

## Overview

Mythrax suffers critical memory issues causing OOM crashes and resource contention during bulk episode ingestion and local model inference. A comprehensive audit + adversarial CTO review identified **21 distinct memory issues** across the ingestion pipeline, model broker, embedding system, search pipeline, and compaction loops. This track remediates all findings, prioritized by severity, and introduces a streaming-to-disk architecture for the cognitive pipeline.

## Root Cause Analysis

Four independent failure modes converge during bulk ingestion:

1. **MLX Lazy Graph Accumulation:** KV cache concatenation and weight dtype casts create unevaluated computation graph nodes that retain all prior tensors in VRAM, growing unboundedly per generated token or model load. The mxbai cross-encoder suffers a similar issue where sequential `as_slice()` calls force duplicate forward passes.
2. **Unbounded Data Materialization:** 24+ call sites execute `SELECT * FROM episode` (and similar) without pagination, loading entire database tables into process memory. The search pipeline's TF-IDF cache-miss path loads all episode *content* into memory, and 3-hop graph traversals produce exponentially-sized result sets.
3. **Async Runtime Starvation:** Six `std::thread::sleep` spin-loops block tokio worker threads while polling Metal GPU semaphores, causing full runtime deadlock under concurrent load.
4. **In-Memory Pipeline Accumulation:** The cognitive pipeline (episodes → summaries → clusters → insights → directions → wisdom) holds all intermediate data in memory during dreaming/compaction cycles. Each stage accumulates Vecs, HashMaps, and large Strings proportional to total data volume, rather than streaming results to vault markdown files as they're generated.

## Functional Requirements

### FR-1: MLX Graph Evaluation (Critical)
- All MLX array operations that store results into caches or weight maps MUST call `.eval()` to materialize the computation graph and release intermediate tensors.
- KV cache updates in `Qwen2Attention::forward` must eval after concatenation.
- Weight dtype casts in `load_model_weights` must eval after `as_dtype`.
- mxbai cross-encoder logits must use joint `mlx_rs::eval(&[&logit_0, &logit_1])` before `as_slice()` access to prevent duplicate forward passes.

### FR-2: Search Pipeline Memory Safety (Critical)
- The TF-IDF IDF cache-miss path MUST NOT load all episode content into memory. Replace with an incremental background IDF indexer that updates term counts as episodes are ingested.
- Temporal neighbor expansion (3-hop `followed_by` traversals) MUST enforce `LIMIT` constraints per hop level.

### FR-3: Paginated Database Queries (High)
- All `get_all_*` query functions must support `LIMIT`/`OFFSET` pagination.
- ALL callers (24+) must migrate from bulk loads to paginated or streaming iteration.
- `get_episodes_by_node_type` must also be paginated.
- Deduplication logic must use hash-based approaches instead of full table scans.

### FR-4: Streaming-to-Disk Cognitive Pipeline (High)
Each stage of the memory pipeline MUST write its output to vault markdown files as produced, then release the in-memory data before proceeding to the next stage. Specific stages:

- **Episodes:** Unprocessed episode chunks must be written to `vault/episodes/*.md` as they're ingested, not accumulated in `Vec<Episode>` across the full batch.
- **Summaries:** Episode summaries produced by the LLM must be flushed to `wiki/<scope>/episodes/*.md` immediately after generation, not held for the full dreaming cycle.
- **Clusters:** DBSCAN cluster assignments must be serialized to a temporary `wiki/<scope>/.clusters/<timestamp>.json` manifest, then individual cluster members read back from disk during synthesis rather than holding all cluster data in-memory.
- **Insights:** Synthesized insight notes must be written to `wiki/<scope>/insights/*.md` immediately and dropped from memory before processing the next cluster.
- **Directions:** Direction promotion results must be written to `wiki/<scope>/directions/*.md` immediately and dropped before evaluating the next candidate.
- **Wisdom:** Graduated wisdom rules must be written to `wisdom/*.md` immediately after cross-scope matching, not accumulated in graduation candidate Vecs.
- **Pruned/Archived:** Pruned leaf nodes, archived episodes, and decay-archived records must be moved to `vault/archive/` and their in-memory references dropped immediately after the move, not held in batch Vecs for later processing.
- **Compaction Summaries:** Compaction cluster summaries must be written to `wiki/<scope>/compactions/*.md` one cluster at a time, releasing the prompt buffer and LLM response after each write.

### FR-5: Non-Blocking Semaphore Acquisition (High)
- Replace all `std::thread::sleep` + `try_acquire()` spin-loops with `tokio::sync::Semaphore` and `.acquire().await`.
- Embedding and inference pipelines must become fully async-compatible.

### FR-6: Embedding Cache Bounded Growth (Critical)
- The binary `flush_dirty` path must not load the entire disk cache into memory.
- Migrate to SQLite-only cache path or implement append-only dirty flushes.
- Cache capacity must be enforced during flush merges.

### FR-7: Daemon Task Lifecycle Management (High)
- All background `tokio::spawn` loops must accept a `CancellationToken`.
- Graceful shutdown must cancel all background tasks before DB/model cleanup.

### FR-8: Startup Backfill Pagination (High)
- Missing-embedding backfill queries at startup must be paginated.
- Process records in bounded batches rather than loading all at once.

### FR-9: Proportional Growth Mitigations (Medium)
- Transcript mining tool sequence: sliding window or periodic flush.
- Forge pipeline: streaming chunk processing.
- API batch endpoint: enforce payload size limits.
- Vault bulk ingestion: use `save_episodes_batch` instead of individual inserts.
- VRAM tracking state desync: update `active_tier` on cache hits.
- Synthesis cluster prompt concatenation: cap at token budget with truncation.

## Non-Functional Requirements

- **Memory Ceiling:** Daemon RSS must stay below 2 GB during bulk ingestion of 1000+ episodes.
- **Pipeline Spill-to-Disk:** No single cognitive pipeline stage may hold more than 50 intermediate results in memory simultaneously.
- **No Deadlocks:** Concurrent embedding + inference workloads must not deadlock the tokio runtime.
- **Backward Compatibility:** No API contract changes. All fixes are internal implementation details.
- **Test Coverage:** >80% coverage on all modified modules.

## Acceptance Criteria

1. `MYTHRAX_TEST_MOCK=1 cargo nextest run` passes with zero failures.
2. Daemon RSS stays below 2 GB during bulk ingestion of 500+ episodes (monitored via `footprint`).
3. 1000-token generation with local Qwen model shows flat VRAM usage (no monotonic increase).
4. `embedding_cache.bin` is migrated to SQLite; binary path removed.
5. All daemon background tasks cleanly terminate on `SIGTERM` within 5 seconds.
6. No `std::thread::sleep` calls remain in async code paths.
7. Each cognitive pipeline stage writes to vault md files incrementally — no stage holds more than 50 items in memory.
8. Search pipeline FTS queries do not load episode content into memory; IDF counts served from pre-computed index.
9. Temporal neighbor expansion returns at most 50 results per hop level.

## Out of Scope

- Model architecture changes (quantization, model selection).
- Database schema migrations or SurrealDB version upgrades.
- New feature development.
- Performance optimization beyond memory safety (e.g., query speed tuning).
