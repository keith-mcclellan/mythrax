# Requirements: Mythrax 2.0

## Problem
The current Mythrax 1.2 memory broker architecture has several limitations:
1.  **Deployment Complexity:** The local LLM backend depends on a separate external server process (Ollama or `mlx-lm` Python server). This increases setup friction, resource overhead, and the risk of connection failures.
2.  **Search Inaccuracy (Semantic Drift):** Pure semantic vector search struggles to retrieve exact technical tokens (variable names, configuration keys, or UUIDs), leading to retrieval drift during debugging tasks.
3.  **Threshold Incompatibility:** The DBSCAN clustering epsilon ($\epsilon$) is hardcoded, causing dreaming to fail or collapse when users switch embedding models with different dimensionalities and similarity distributions.
4.  **Orchestration Overhead:** The local model struggles to execute complex TDD task handoffs directly due to instruction-following limitations, resulting in invalid tool calls or compilation failures.
5.  **VRAM & Disk Safety:** Frequent swapping of large local models into the system SSD swap space risks disk thrashing, system-level out-of-memory (OOM) crashes, and accelerated SSD wear.
6.  **C++ Compilation Dependency:** Relying on RocksDB requires a C++ compiler toolchain, making cross-compilation fragile.

## Outcome
A self-contained, 100% pure-Rust, highly portable daemon (Mythrax 2.0) that hosts its own GPU-accelerated local LLM and embedding models on Apple Silicon (via native MLX) and falls back to ONNX or external APIs on other platforms. The daemon performs hybrid RRF searches with soft-thresholding, self-calibrates clustering thresholds, automates TDD tree search execution, and provides a single-port, zero-friction path-based routing gateway.

## User Value
*   **True Offline Autonomy:** 100% private, air-gapped memory and local inference with zero external cloud or server dependencies.
*   **Zero-Friction Integration:** Integrates instantly with Antigravity, OpenCode, or OpenClaw using a single environment variable and a secure, persistent local API key.
*   **Exceptional Performance:** Sub-millisecond database queries, zero IPC model latency, split-second embedding generations, and >60 tokens/sec local inference.
*   **Bulletproof Stability:** Complete protection against database corruption, port conflicts, VRAM leaks, SSD swap thrashing, and disk wear.

## In Scope
*   **3-Tiered Model Architecture & Dynamic Scheduling:** Standardize on a 3-tiered local model execution model to optimize between interactive speed, memory safety, and deep reasoning capabilities:
    *   *Tier 1 (Quick Inference / Embeddings):* Pinned `nomic-embed-text-v1.5-mlx` (~100MB) for vector generation plus a lightweight LLM (default `Qwen2.5-1.5B-Instruct-MLX-4bit` or `Llama-3.2-3B-Instruct`, ~1-2GB) pinned/highly-reactive for fast classification, RAG scoring, and symbolic routing (<50ms).
    *   *Tier 2 (Active Coding / TDD):* Dynamically loaded/swapped LLM (default `Qwen2.5-Coder-7B-Instruct-MLX-4bit`, ~4.5GB) for code generation, test creation, and stateful TDD compiler runs. Swaps in <1.5 seconds, keeping the interactive VRAM footprint under 6GB to prevent system SSD swap thrashing.
    *   *Tier 3 (Deep Inference / Dream Synthesis):* High-reasoning LLM (default `mlx-community/Qwen3.6-35B-A3B-4bit`, ~20GB) loaded **exclusively during idle-gated dreaming cycles** (when the user is inactive). It performs Ebbinghaus decay compaction, long-term wisdom extraction, and memory graph synthesis, and is completely evicted (triggering Metal cache purges) immediately upon dreaming completion or user-activity detection.
*   **ONNX Cross-Platform Parity & Dynlib Linkage:** Explicitly define and support an ONNX target profile across the 3 tiers (using equivalent ONNX models like `onnx-community/Qwen2.5-Coder-7B-Instruct-ONNX` or `onnx-community/Qwen3.6-35B-A3B-ONNX` depending on the active tier) with model resolution, download, and caching via the `ort` crate (with the `download-binaries` feature enabled in `Cargo.toml` to automatically download and link pre-compiled ONNX Runtime shared libraries).
*   **Split GPU Semaphores:** Establish `METAL_INFERENCE_SEMAPHORE` (capacity 1) and `METAL_EMBEDDING_SEMAPHORE` (capacity 1) to allow high-frequency embeddings and long completions to run concurrently without GPU driver panics.
*   **Upstream Watcher Filtering, 500ms Coalescing & Bounded Worker Pool:** Filter out `.trash/`, `target/`, and `.git/` directories directly inside the notify event handler callback, preventing channel congestion. Implement a **500ms write-behind debouncing queue** inside `watcher.rs` to consolidate high-frequency file modifications. Route all background embedding tasks through a **bounded process-global channel (worker pool)** to limit concurrent background embedding generations to exactly 1 or 2 tasks, preventing thread pool saturation. Disable following symlinks, and restrict watch depth to 10 levels and 50,000 files.
*   **Thread-Safe Episodic WAL Actor, Robust Parsing & Compaction:** Implement an asynchronous channel-based WAL actor: all saves write to a `tokio::sync::mpsc::channel`, and a single background task sequentially appends and flushes JSONL records to `~/.mythrax/episodes.jsonl`. On WAL recovery replay, read the file line-by-line, logging a warning and skipping malformed lines. Implement a **WAL log compaction step** during the background dreaming cycle that reads `episodes.jsonl` and retains only the latest version of each unique episode ID, preventing indefinite file growth.
*   **Persistent Local Token & Owners-Only Permissions:** Read `~/.mythrax/token` on startup. If the file exists and contains a valid key, reuse it to preserve session continuity. Only generate a new token if the file is missing or invalid, writing it with strict Unix file permissions (`0600` / owner read-write only).
*   **SurrealKV Table History & Pruning:** Define table schemas with a custom `wiki_node_history` table and database event triggers (`DEFINE EVENT`) to record historical versions. Implement a background history pruning policy inside the dreaming compaction cycle to automatically delete history records older than 30 days to prevent table bloat.
*   **Episodic WAL Recovery Replay with Marker File:** Replay the episodic JSONL WAL log file into SurrealKV during startup recovery *only* if a missing `.initialized` marker file inside the SurrealKV directory indicates a fresh database recreation.
*   **Database Migration Coordinator:** Track `schema_version` on boot and execute versioned SQL scripts or trigger automated re-indexing on major version mismatches.
*   **Soft Threshold RRF:** Implement a soft-thresholding function (sigmoid scaling) and pass a larger candidate pool to the RRF fusion step, preserving semantic signals that fall just below the hard threshold.
*   **Token-Level Preemption:** Check the cooperative `CancellationToken` inside the FFI token-generation loop after every single token, enforcing a 2-second hard preemption timeout.
*   **Compliant Stream Interceptor:** Format the injected `Execution Check` block as valid, OpenAI-compliant SSE JSON chunks (`choices[0].delta.content`) to prevent client SDK crashes.
*   **Single-Port Path-Based Routing:** Consolidate daemon and completions proxy onto a single port (8090). Route proxy requests to `/v1/*` and `/api/*`, and route daemon management, health, and telemetry to `/mythrax/*`.
*   **Safe Local Archive:** Restrict the daemon from writing new Markdown files back to the vault root. Write historical digests to a local archive directory (`~/.mythrax/archive/`) exposed via the API.
*   **Direct Command Execution:** Spawn compiler and test commands directly using Rust's `std::process::Command` (passing arguments as separate array elements), avoiding command injection vulnerabilities.
*   **Pre-Inference Shader Warm-up & Cache Panic Fallback:** Execute a single-token dummy inference step immediately after model swaps to compile and cache Metal shader kernels. Wrap the warm-up call in safe error handling, logging a warning and falling back to CPU-only execution if Metal shader cache corruption occurs.
*   **Canonicalized Pre-Download Disk Check:** Resolve the target download directory using `fs::canonicalize` prior to calling `libc::statfs` on macOS, verifying free disk space exceeds the model size plus a **10.0 GB** safety buffer on the correct partition mount point.
*   **Model-Aware Epsilon Calibration Override:** Run dynamic DBSCAN calibration on startup, falling back to a pre-defined model family default if embeddings are $<100$, and allow users to override the fallback epsilon via `mythrax.yaml` (`embeddings.default_epsilon`).
*   **Rolling Log Appender:** Use `tracing-appender` to roll `daemon.log` at 50MB, maintaining a maximum of 3 historical backups to prevent disk exhaustion.
*   **Graceful Shutdown Timeout:** Wrap the shutdown sequence in a 5-second timeout, forcing exit if it hangs.
*   **CI/CD Non-macOS Compilation:** Annotate all Metal/MLX-dependent integration tests with `#[cfg(feature = "mlx")]` to ensure they compile and are skipped on non-macOS CI/CD runners.
*   **Zero Hardcoded System & Cache Parameters:** All systems, memory, caching, and performance parameters must be fully configurable in `config.json` or `mythrax.yaml` to allow advanced users on capable hardware (e.g. Mac Studio with 192GB RAM) to maximize throughput:
    *   *Memory/Swap:* Support `memory.disable_swap_monitor` (boolean), and `memory.swap_threshold_tier1_gb`, `memory.swap_threshold_tier2_gb`, and `memory.swap_threshold_tier3_gb` (floats) to raise or disable active swap monitors entirely.
    *   *Context Window:* Support `model.max_context_window` (integer) to expand contexts beyond the default 16k tokens.
    *   *Concurrency:* Support `embeddings.max_concurrent_tasks` (integer) to scale background indexing threads from the default 2 up to hardware limits.
    *   *HTR TDD Loop:* Support `htr.tdd_max_attempts` (integer) and `htr.preemption_timeout_seconds` (float) to tune stateful tree search parameters.
    *   *Compaction:* Support `compaction.decay_threshold` (float), `compaction.history_pruning_days` (integer), and `compaction.wal_compaction_hours` (integer) to tune memory forgetting, time-travel history retention, and transaction log compaction intervals.

## Out of Scope
*   Creating a frontend Obsidian community plugin (TS/JS codebase).
*   Adding cryptographic Macaroon/JWT handoff tokens (deferred to Phase 3).
*   Implementing custom, non-standard LLM architectures not supported by `mlxrs` or `ort`.

## Constraints
*   **macOS Lock-in for MLX:** Native MLX execution is strictly macOS-only. Cross-platform builds must compile cleanly without MLX and fall back to ONNX or external APIs.
*   **Pre-Download Disk Space Buffer:** A minimum of 10.0 GB of free disk space is required before any model download starts.
*   **Memory Pressure Monitor:** Evict models and clear caches if macOS memory pressure exceeds 80% or free RAM falls below 500MB, avoiding rigid swap-based thresholds.
*   **Single-Port Routing:** The entire system must bind and operate on a single port (8090).
*   **API Token Security:** All HTTP endpoints must require the generated local API key.

## Risks and Edge Cases
*   *Metal Driver FFI Crash:* GPU memory allocation failure or concurrent queue submission panics the Metal driver, crashing the daemon. *Mitigation:* The `mythrax exec` CLI wrapper detects daemon termination, intercepting connection failures and dynamically spawning a fallback CPU daemon or redirecting to the cloud.
*   *Offline Model Swap Failure:* A model swap is triggered while offline and the target model is uncached. *Mitigation:* The broker performs offline verification, gracefully keeping the active model loaded and modifying the prompt context to fit the active model's capacity.
*   *Symlink loops:* Watched directories containing circular references or massive build folders. *Mitigation:* Disable following symlinks in `watcher.rs` and restrict watch depth to a maximum of 10 levels and 50,000 files.
*   *Stale Lock PID Reuse:* Checking only if the PID is active in the OS is vulnerable to PID recycling. *Mitigation:* Verify both the PID and the process name (`mythrax` or `mythrax-core`) during the boot liveness check.

## Acceptance Criteria
*   **A1 [Pure Rust Compilation]:** `cargo build` compiles `mythrax-core` successfully on macOS, Linux, and Windows with zero C++ compiler (`clang`/`gcc`) requirements for the database backend, downloading pre-compiled ONNX Runtime shared libraries via `ort`.
*   **A2 [In-Process Inference]:** The daemon resolves, downloads, and loads default MLX models natively on macOS, executing GPU-accelerated inference in-process with zero external server dependencies.
*   **A3 [Hybrid RRF Search]:** Vector and BM25 search queries are executed in parallel in SurrealKV, and their results are successfully merged in-memory using Reciprocal Rank Fusion ($k=60$).
*   **A4 [Dynamic Calibration with Overridable Fallback]:** The k-distance elbow method successfully calculates and caches the optimal epsilon ($\epsilon$) per scope on startup, falling back to a default mapped to the active model family (or overridden via `mythrax.yaml`) if embeddings are $<100$.
*   **A5 [Stateful TDD Tree Search]:** The HTR engine executes the Test-Compile-Implement-Verify cycle as a tree search, isolating target directories, running offline, and terminating cleanly after 3-5 failed compile attempts with a fully compiled `Diagnostic Post-Mortem`.
*   **A6 [Smart Stream Interception]:** All proxy completions are prepended with a validated, OpenAI-compliant SSE JSON stream chunk, which is silently generated and injected by the Axum gateway if the model fails to output it.
*   **A7 [Active Forgetting, History Pruning & Safe Archive]:** Compacts decayed nodes into a local archive folder in `~/.mythrax/archive/`, and prunes history records older than 30 days during dreaming to prevent table bloat.
*   **A8 [Persistent Redirection & Strict Token Auth]:** Spawning `mythrax exec -- antigravity run` injects the persistent API token (loaded securely with `0600` permissions), ensuring session continuity across daemon restarts.
*   **A9 [Defensive Memory Management & Dynamic Swap Monitoring]:** Swapping models triggers explicit Metal cache eviction, and high memory pressure or low free RAM immediately halts active tasks and clears VRAM to prevent thrashing. The active swap monitor thresholds dynamically adjust based on the loaded model tier (e.g., eviction triggers at 2.0 GB of active swap for the 1.5B Tier 1 LLM, 3.0 GB of active swap for the 7B Tier 2 Coder model, and 6.0 GB for the 35B Tier 3 Deep Reasoning model).
*   **A10 [Non-Blocking Runtime]:** The Axum HTTP server remains fully responsive (queries returning $<50\text{ ms}$) during active background batch processes, and concurrent CLI commands do not spawn duplicate daemon processes.
*   **A11 [Self-Healing & Single-Port Roaming]:** Stale PID/lock files are cleaned on startup, and port conflicts are healed by dynamically roaming to open ports and writing them to `config.json`, checking process name to prevent PID recycling issues.
*   **A12 [100% Automated Verification]:** All unit, integration, and E2E tests are executed automatically and pass successfully during `cargo test`.
*   **A13 [Zero VRAM Eviction Spill]:** The model broker blocks new model allocations until the strong reference count of the evicted model is exactly 0, preventing concurrent allocation spikes.
*   **A14 [Upstream Watcher Filtering, Coalescing & Bounded Pool]:** Upstream path filtering inside the notify callback discards ignored directories, a 500ms debouncing write-behind queue coalesces high-frequency edits, and a bounded worker pool limits concurrent background embedding generations to exactly 1 or 2 tasks.
*   **A15 [Split GPU Semaphores]:** Separate inference and embedding semaphores serialize tasks by type, allowing concurrent embeddings and completions to run without Metal queue collisions.
*   **A16 [HTR Registry Lock Immunity]:** HTR tree search compilations run in offline mode (`--offline`), preventing cargo registry lock contentions during parallel builds.
*   **A17 [Robust Episodic WAL, Replay Marker & Compaction]:** Episodic memories are backed up in an append-only JSONL log via a thread-safe async appender channel, replayed on startup recovery only if a missing `.initialized` marker file indicates fresh database recreation (skipping malformed JSON lines), and compacted every 24 hours during dreaming.
*   **A18 [Time-Travel Schema History Triggers]:** The database schema defines a `wiki_node_history` table and triggers that record historical versions, enabling robust point-in-time queries.
*   **A19 [Command Injection Prevention]:** The CLI wrapper executes processes directly using `std::process::Command` without spawning a shell, eliminating command injection risks.
*   **A20 [Canonicalized Pre-Download Disk Check]:** The model resolver verifies a minimum of 10.0 GB of free disk space on the canonicalized target partition before downloading any model weights, aborting with a clear error on failure.
