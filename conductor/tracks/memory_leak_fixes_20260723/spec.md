# Specification: Memory Leak Remediation & OOM Crash Prevention

## Overview

Mythrax suffers critical memory issues causing OOM crashes and resource contention during bulk episode ingestion and local model inference. A comprehensive audit identified **17 distinct memory issues** across the ingestion pipeline, model broker, embedding system, and compaction loops. This track remediates all findings, prioritized by severity.

## Root Cause Analysis

Three independent failure modes converge during bulk ingestion:

1. **MLX Lazy Graph Accumulation:** KV cache concatenation and weight dtype casts create unevaluated computation graph nodes that retain all prior tensors in VRAM, growing unboundedly per generated token or model load.
2. **Unbounded Data Materialization:** 12+ call sites execute `SELECT * FROM episode` (and similar) without pagination, loading entire database tables into process memory.
3. **Async Runtime Starvation:** Six `std::thread::sleep` spin-loops block tokio worker threads while polling Metal GPU semaphores, causing full runtime deadlock under concurrent load.

## Functional Requirements

### FR-1: MLX Graph Evaluation (Critical)
- All MLX array operations that store results into caches or weight maps MUST call `.eval()` to materialize the computation graph and release intermediate tensors.
- KV cache updates in `Qwen2Attention::forward` must eval after concatenation.
- Weight dtype casts in `load_model_weights` must eval after `as_dtype`.

### FR-2: Paginated Database Queries (High)
- All `get_all_*` query functions must support `LIMIT`/`OFFSET` pagination.
- Callers must migrate from bulk loads to paginated or streaming iteration.
- Deduplication logic must use hash-based approaches instead of full table scans.

### FR-3: Non-Blocking Semaphore Acquisition (High)
- Replace all `std::thread::sleep` + `try_acquire()` spin-loops with `tokio::sync::Semaphore` and `.acquire().await`.
- Embedding and inference pipelines must become fully async-compatible.

### FR-4: Embedding Cache Bounded Growth (Critical)
- The binary `flush_dirty` path must not load the entire disk cache into memory.
- Migrate to SQLite-only cache path or implement append-only dirty flushes.
- Cache capacity must be enforced during flush merges.

### FR-5: Daemon Task Lifecycle Management (High)
- All background `tokio::spawn` loops must accept a `CancellationToken`.
- Graceful shutdown must cancel all background tasks before DB/model cleanup.

### FR-6: Startup Backfill Pagination (High)
- Missing-embedding backfill queries at startup must be paginated.
- Process records in bounded batches rather than loading all at once.

### FR-7: Proportional Growth Mitigations (Medium)
- Transcript mining tool sequence: sliding window or periodic flush.
- Forge pipeline: streaming chunk processing.
- API batch endpoint: enforce payload size limits.
- Vault bulk ingestion: use `save_episodes_batch` instead of individual inserts.
- VRAM tracking state desync: update `active_tier` on cache hits.

## Non-Functional Requirements

- **Memory Ceiling:** Daemon RSS must stay below 2 GB during bulk ingestion of 1000+ episodes.
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

## Out of Scope

- Model architecture changes (quantization, model selection).
- Database schema migrations or SurrealDB version upgrades.
- New feature development.
- Performance optimization beyond memory safety (e.g., query speed tuning).
