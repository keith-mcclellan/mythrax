# Tasks: Mythrax 2.0 (Phased Delivery Plan)

This document organizes the 16 atomic tasks into four sequential, dependency-gated delivery phases. Each task contains explicit AST modifications, step-by-step instructions, and verification commands, enabling independent execution under a strict TDD loop.

---

## Phase 1: Core Engine & Storage (Foundation)
*Focus: Transition database to SurrealKV, establish FFI safety, compile-warm Metal shaders, dynamically resolve stop tokens, handle stale locks, and define the time-travel history triggers.*

### T1: Dependency Refactoring, SurrealKV & ONNX Parity
*   **AST Specification:**
    *   **[MODIFY]** `mythrax-core/Cargo.toml`: Replace RocksDB features with SurrealKV (`features = ["kv-surrealkv", "kv-mem"]`), add `rlimit` crate. Add `download-binaries` feature to the `ort` dependency (`ort = { version = "2.0.0-rc.9", features = ["download-binaries"] }`) to automatically resolve ONNX Runtime shared library dynamic linkage in non-macOS environments.
    *   **[MODIFY]** `src/db/backend.rs`: Replace `RocksDb` with `SurrealKv`. Wrap write transactions with a serialized write-queue and an exponential backoff retry loop (handling `TransactionConflict` cleanly).
*   **Handoff Contract:** Connect SurrealDB to the pure-Rust SurrealKV engine. Ensure ONNX dynamic libraries are automatically downloaded and linked via `ort` on cross-platform developer boxes.
*   **Verification:** `cargo test --test test_non_blocking_daemon`

### T2: Low-Level Systems Safeguards, Stale Lock Guard & Active Swap Monitor
*   **AST Specification:**
    *   **[MODIFY]** `src/main.rs`: Query soft file descriptor limit on boot and programmatically raise it to the hard limit (typically 10,240 on macOS) using the `rlimit` crate.
    *   **[MODIFY]** `src/main.rs`: Implement stale lock recovery. Check PID in `~/.mythrax/daemon.pid`; to prevent PID recycling issues (where an unrelated recycled process matches the PID), the liveness check must verify that the process running at that PID has the name `mythrax` or `mythrax-core`. If it is inactive or named differently, purge `daemon.pid` and `daemon.lock` before booting.
    *   **[MODIFY]** `src/main.rs`: Spawn a background active swap monitor thread running every 10 seconds. It must query host swap usage (via `sysctl vm.swapusage` on macOS) and enforce model-aware swap thresholds read dynamically from configuration (`memory.swap_threshold_tier1_gb`, `memory.swap_threshold_tier2_gb`, `memory.swap_threshold_tier3_gb`, completely bypassing monitor checks if `memory.disable_swap_monitor` is `true`), triggering background task suspension, Metal cache eviction, and model unloading to prevent host memory thrashing.
*   **Handoff Contract:** Prevent file descriptor exhaustion during heavy indexing, self-heal from crashed locks without recycled PID collisions, and protect host stability from out-of-memory thrashing using configurable, model-aware swap limits.
*   **Verification:** `cargo test --test test_non_blocking_daemon`

### T3: 3-Tiered Model Broker & VRAM Swapping
*   **AST Specification:**
    *   **[MODIFY]** `src/llm/mod.rs`: Implement a 3-tiered model broker, loading model paths dynamically from configuration (`models.embeddings`, `models.tier1_fast_llm`, `models.tier2_coder_llm`, and `models.tier3_reasoning_llm`) and enforcing configurable context windows (`models.max_context_window`):
        - *Tier 1:* Pinned embedding model + a lightweight classification LLM loaded permanently/highly-reactive.
        - *Tier 2:* Dynamic coding LLM loaded reactively for completions and HTR TDD tasks.
        - *Tier 3:* High-reasoning LLM loaded exclusively during idle-gated dreaming cycles and evicted immediately on completion or user activity.
    *   **[MODIFY]** `src/llm/mod.rs`: Implement `Weak<dyn InferenceEngine>` tracking. Ensure that dropping the strong reference automatically unloads model weights and calls `mlxrs::metal::clear_cache()` to purge physical memory.
*   **Handoff Contract:** Provide sequentially isolated model loading across the three tiers reading paths dynamically from config, dynamic weight swapping, and automatic weak-pointer VRAM eviction.
*   **Verification:** `cargo test --test test_model_broker`

### T14: Time-Travel History Trigger Integration
*   **AST Specification:**
    *   **[MODIFY]** `src/db/schema.rs`: Update the `INIT_SCHEMA` script to explicitly define a `wiki_node_history` table and a database trigger event (`DEFINE EVENT`) that automatically inserts historical versions into `wiki_node_history` on update.
*   **Handoff Contract:** Automate version archiving at the database schema layer to enable robust, point-in-time time-travel queries.
*   **Verification:** `cargo test --test test_non_blocking_daemon`

### T15: FFI Send/Sync Safety & Token-Level Preemption
*   **AST Specification:**
    *   **[MODIFY]** `src/llm/mod.rs`: Declare `unsafe impl Send` and `unsafe impl Sync` for `InProcessMlxEngine`.
    *   **[MODIFY]** `src/llm/mod.rs`: Parse `tokenizer_config.json` dynamically on load to extract `eos_token` and `stop_sequences` dynamically.
    *   **[MODIFY]** `src/llm/mod.rs`: Implement token-level preemption by checking the cooperative `CancellationToken` inside the FFI token-generation loop after every single token, enforcing a configurable preemption timeout (`htr.preemption_timeout_seconds`, defaulting to `2.0` seconds) read dynamically from configuration.
*   **Handoff Contract:** Guarantee thread-safe completions execution, dynamic stop token safety, and rapid dreaming preemption.
*   **Verification:** `cargo test --test test_model_broker`

---

## Phase 2: Search, Ingestion & Watcher (Data Flow)
*Focus: Implement parallel hybrid search with soft-threshold RRF, configure upstream file watcher ignore filtering, and integrate the episodic WAL recovery.*

### T4: Hybrid RRF Search, Soft Thresholds & Decay
*   **AST Specification:**
    *   **[MODIFY]** `src/db/backend.rs`: Implement parallel vector (HNSW) and full-text (BM25) search queries.
    *   **[MODIFY]** `src/db/backend.rs`: Implement a soft-thresholding function (sigmoid scaling) and pass a larger candidate pool to the Reciprocal Rank Fusion ($k=60$) fusion step, preserving semantic signals that fall just below the hard threshold.
    *   **[MODIFY]** `src/cognitive/compactor.rs`: Implement Ebbinghaus temporal decay calculations ($e^{-\lambda t}$) based on `last_retrieved_at` timestamps, loading the compaction decay threshold (`compaction.decay_threshold`, default `0.15`) dynamically from configuration.
*   **Handoff Contract:** Execute and merge semantic and keyword searches cleanly using RRF, preserving borderline semantic matches and using configurable decay thresholds.
*   **Verification:** `cargo test --test test_pre_invocation_hook`

### T5: Dynamic Epsilon Calibration with Config Overrides
*   **AST Specification:**
    *   **[MODIFY]** `src/cognitive/synthesis.rs`: Implement DBSCAN $\epsilon$ calibration using the k-distance elbow method over a sample of 100 database embeddings.
    *   **[MODIFY]** `src/cognitive/synthesis.rs`: Implement a safe fallback: if the database has fewer than 100 embeddings, bypass calibration and load a pre-defined model family fallback (or the user-defined override `embeddings.default_epsilon` configured in configuration, defaulting to `0.55`).
*   **Handoff Contract:** Automate threshold calibration on boot while ensuring safe, overridable fallbacks in fresh environments.
*   **Verification:** `cargo test --test test_non_blocking_daemon`

### T11: Upstream Watcher Symlink, Depth, Coalescing & Bounded Pool Guards
*   **AST Specification:**
    *   **[MODIFY]** `src/vault/watcher.rs`: Refactor the file watcher to perform path ignore filtering (`.trash`, `target`, `.git`, `.mythrax`) directly inside the notify event handler callback, discarding ignored paths before they enter the tokio channel. 
    *   **[MODIFY]** `src/vault/watcher.rs`: Explicitly disable following symbolic links. Set a hard monitor limit: maximum recursion depth of 10 levels and maximum watch limit of 50,000 files.
    *   **[MODIFY]** `src/vault/watcher.rs`: Implement a **500ms write-behind debouncing queue**. Rapid modifications to a note inside 500ms must be coalesced and committed to the database as a single write event.
    *   **[MODIFY]** `src/vault/watcher.rs`: Route all background embedding tasks through a **bounded process-global channel (worker pool)** that limits concurrent background embedding generations dynamically to `embeddings.max_concurrent_tasks` read from configuration (default `2`). This ensures bulk file edits (like git checkouts) are processed sequentially in the background, preventing thread pool starvation and keeping RAG queries highly responsive.
*   **Handoff Contract:** Protect the watcher channel, daemon thread, and host file descriptors from congestion, circular symlink loops, high-frequency edit storms, and thread pool starvation using configurable concurrency bounds.
*   **Verification:** `cargo test --test test_watcher_stress`

---

## Phase 3: Concurrency, Memory & Swapping (Safety)
*Focus: Split GPU semaphores, implement swap stream synchronization gates, pre-download disk checks, and pre-inference shader warm-ups.*

### T12: Swap Stream Synchronization
*   **AST Specification:**
    *   **[MODIFY]** `src/llm/mod.rs`: Add a swap synchronization gate inside the `DynamicModelBroker` that tracks active completions streams and blocks new model allocations until the evicted model's strong reference count is exactly 0.
*   **Handoff Contract:** Eliminate concurrent model allocations in VRAM, preventing out-of-memory spikes and system thrashing.
*   **Verification:** `cargo test --test test_model_broker`

### T13: Split GPU Semaphores
*   **AST Specification:**
    *   **[MODIFY]** `src/llm/mod.rs`: Replace the single global semaphore with two distinct semaphores: `METAL_INFERENCE_SEMAPHORE` (capacity 1) and `METAL_EMBEDDING_SEMAPHORE` (capacity 1).
*   **Handoff Contract:** Allow high-frequency embedding generations and long-running completions streams to execute concurrently without graphics driver panics.
*   **Verification:** `cargo test --test test_non_blocking_daemon`

### T17: Async WAL Actor, Robust Recovery Parsing, Replay Marker & Compaction
*   **AST Specification:**
    *   **[MODIFY]** `src/db/backend.rs`: Establish an asynchronous `tokio::sync::mpsc::channel` for writing to the write-ahead log (`~/.mythrax/episodes.jsonl`).
    *   **[MODIFY]** `src/db/backend.rs`: Spawn a single, dedicated background tokio task (actor) that reads `EpisodeSave` payloads from the channel and appends/flushes them sequentially to `episodes.jsonl` (using strict `0600` permissions on file creation).
    *   **[MODIFY]** `src/db/backend.rs`: Implement robust recovery parsing: read the WAL file line-by-line. If a line is malformed or contains invalid JSON (due to sudden power failure), **log a warning, skip the corrupted line, and continue replaying remaining records** without aborting.
    *   **[MODIFY]** `src/db/backend.rs`: Replay the JSONL WAL into SurrealKV on boot *only* if the `.initialized` marker file is missing inside the database directory, writing the marker file immediately on successful replay.
    *   **[MODIFY]** `src/db/backend.rs`: Implement WAL compaction: during the background dreaming compaction cycle, read the append-only `episodes.jsonl` file, retain only the latest version of each unique episode ID, and rewrite a compacted, slimmed-down WAL file. This compaction cycle must be triggered at configurable intervals (`compaction.wal_compaction_hours`, defaulting to `24` hours) read dynamically from configuration to prevent indefinite file growth.
*   **Handoff Contract:** Prevent concurrent write corruption, redundant startup write overhead, WAL recovery aborts, and indefinite WAL file growth, ensuring durable episodic memories.
*   **Verification:** `cargo test --test test_non_blocking_daemon`

### T18: Canonicalized Disk Check, Shader Warm-up & Cache Panic Fallback
*   **AST Specification:**
    *   **[MODIFY]** `src/llm/mod.rs`: Implement `check_disk_space` in the downloader. Canonicalize the target path via `fs::canonicalize` prior to calling `libc::statfs` on macOS, verifying a minimum of 10.0 GB of free space on the correct partition.
    *   **[MODIFY]** `src/llm/mod.rs`: Implement `warm_up_shaders`: after loading a model, execute a single-token dummy inference step to compile and cache Metal shader kernels. Wrap this FFI execution in a safe error handler (catching panics/FFI compile errors). If the FFI warm-up panics, log a critical warning and gracefully **fall back to CPU-only execution mode** rather than crashing.
*   **Handoff Contract:** Prevent SSD virtual memory crashes from low disk space on symlinked mount points, eliminate cold-start inference latencies, and protect against Metal shader cache corruption crashes.
*   **Verification:** `cargo test --test test_model_broker`

---

## Phase 4: APIs, Integration & Lifecycle (Gateways)
*Focus: Implement the single-port path-based router, secure completions proxy with local API keys, inject compliant JSON stream chunks, enforce direct command executions, roll log files, and register graceful shutdowns.*

### T6: HTR TDD Tree Engine & Offline Builds
*   **AST Specification:**
    *   **[MODIFY]** `src/cognitive/arbor.rs`: Integrate the stateful TDD loop (Test -> Compile -> Implement -> Verify) inside the HTR engine, reading the maximum failed compilation/test attempts before halting (`htr.tdd_max_attempts`, defaulting to `5` attempts) dynamically from configuration.
    *   **[MODIFY]** `src/cognitive/arbor.rs`: Run all Cargo test compilations in offline mode (`cargo test --offline`), and set a unique `CARGO_TARGET_DIR` per spawned process.
*   **Handoff Contract:** Execute isolated, offline TDD compiles as a tree search, escalating to a Diagnostic Post-Mortem on failure.
*   **Verification:** `cargo test --test test_tdd_escalation`

### T7: Transparent Proxies & Compliant Stream Interception
*   **AST Specification:**
    *   **[MODIFY]** `src/api.rs`: Refactor the completions proxy. If the model fails to output the `Execution Check` block, the Axum middleware must silently format and inject it as valid, OpenAI-compliant SSE JSON stream chunks (`choices[0].delta.content`).
*   **Handoff Contract:** Intercept completions streams securely and prepend compliant execution checks without crashing standard client SDKs.
*   **Verification:** `cargo test --test test_cli_redirection`

### T8: Persistent Token Gateway & Command Safety
*   **AST Specification:**
    *   **[MODIFY]** `src/cli.rs`: Refactor token loading: on startup, check if `~/.mythrax/token` exists and contains a valid key. Reuse it to preserve session continuity for active child processes if the daemon restarts. Only generate a new token and overwrite the file if missing or invalid (created with strict owner-only `0600` permissions).
    *   **[MODIFY]** `src/cli.rs`: Refactor `mythrax exec` to spawn the target child process directly using Rust's `std::process::Command` (passing arguments as separate array elements, avoiding shell commands). Validate Bearer token headers on all daemon HTTP requests.
*   **Handoff Contract:** Inject redirect environment variables securely, block command injection vulnerabilities, preserve session token continuity, and protect the gateway from unauthorized local access.
*   **Verification:** `cargo test --test test_cli_redirection`

### T10: Pre-Invocation Hook & Context Injection
*   **AST Specification:**
    *   **[MODIFY]** `src/mcp_routes.rs`: Refactor the pre-invocation hook handler to query the model broker and append a formatted local inference and model status markdown block.
*   **Handoff Contract:** Inject active model and GPU status blocks directly into the calling agent's prompt context.
*   **Verification:** `cargo test --test test_pre_invocation_hook`

### T16: Single-Port Routing, Rolling Logs, Pruning & Graceful Shutdown
*   **AST Specification:**
    *   **[MODIFY]** `src/daemon.rs`: Consolidate completions proxy and daemon routes onto a single port (8090).
    *   **[MODIFY]** `src/daemon.rs`: Implement a rolling file appender using `tracing-appender` to roll `daemon.log` at 50MB, maintaining a maximum of 3 historical backups to prevent disk exhaustion.
    *   **[MODIFY]** `src/daemon.rs`: Implement a background history table pruning transaction to delete `wiki_node_history` records older than `compaction.history_pruning_days` (defaulting to `30` days) read dynamically from configuration during the dreaming compaction cycle.
    *   **[MODIFY]** `src/daemon.rs`: Implement `trigger_graceful_shutdown` wrapping the queue flushing, VRAM eviction, Metal cache purging, and lock cleanup in a 5-second `tokio::time::timeout`. Hook into tokio's `ctrl_c()` signal listener.
*   **Handoff Contract:** Provide single-port simplicity, rolling log safety, history table pruning, and transactional, timeout-guarded graceful shutdowns.
*   **Verification:** `cargo test --test test_graceful_shutdown`
