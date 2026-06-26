# Clarify: Mythrax 2.0 Architecture

## Restated Request
Refactor the Mythrax daemon and memory system into a self-contained, high-performance sidecar intelligence (Mythrax 2.0). 
The core deliverables are:
1.  **Native Inference & Embeddings:** Standardize on Apple Silicon GPU-accelerated local execution (via native `mlxrs`/`mlxcel` bindings) for both text generation and embeddings, eliminating the ONNX Runtime (`ort`) dependency on macOS, while maintaining cross-platform fallbacks (ONNX GenAI or external APIs).
2.  **Hybrid Search:** Implement parallel vector and full-text (BM25) search in SurrealDB 3.x with Reciprocal Rank Fusion (RRF) and soft-threshold candidate filtering.
3.  **Dynamic Calibration:** Automate DBSCAN threshold ($\epsilon$) calibration using the k-distance elbow method on startup, with safe pre-defined fallbacks for fresh installs.
4.  **Stateful HTR TDD Engine:** Move TDD execution (Test -> Compile -> Implement -> Verify) directly into the Cognitive Hypothesis Tree Search (HTR) pipeline, isolating parallel branch compilations and compiling offline.
5.  **Smart Stream Interceptor:** Prepend a validated, OpenAI-compliant `Execution Check` block to the completions stream using Axum middleware.
6.  **Active Memory Forgetting & Append-Only WAL:** Implement Ebbinghaus decay, compact highly decayed nodes ($<0.15$) into a local archive folder (avoiding vault writes), and write episodic memories to an append-only JSONL write-ahead log for crash recovery.
7.  **Zero-Friction Integration:** Consolidate daemon and completions proxy onto a single port (8090) with path-based routing, and provide a unified CLI wrapper (`mythrax exec`) using direct execution (bypassing shell command injection).
8.  **Defensive Memory Guardrails:** Implement explicit Metal cache eviction, swap stream synchronization gates, process-global split GPU semaphores, and memory pressure monitoring.
9.  **Non-Blocking & Concurrent Daemon:** Run heavy tasks asynchronously on background threads, implementing transaction retry loops and PID-guarded duplicate spawn prevention.
10. **Rust-Native Embedded Database:** Transition the persistent storage backend from RocksDB to SurrealKV to achieve 100% pure-Rust compilation, defining tables with explicit changefeeds and history tables for time-travel queries.
11. **100% Automated E2E Verification:** Move all verification steps into a robust, automated integration test suite under `tests/` with clean CI/CD non-macOS compile targets.

## Known Facts
*   `mythrax-core` is written in Rust (v2024 edition) and runs a background Axum HTTP daemon on port 8090.
*   The current persistent database is SurrealDB 3.1.5 running on RocksDB.
*   The current local LLM completions are executed via an external local server on port 8080.
*   The current embedding model runs in-process using the ONNX Runtime (`ort` crate).
*   The host machine is running macOS.
*   The target agentic framework is Antigravity.

## Assumptions
*   The developer's machine is an Apple Silicon Mac, making Metal-accelerated MLX the optimal path.
*   The Markdown files in the Obsidian vault are the absolute source of truth; the database serves as a high-performance query index.
*   A 2-tiered model architecture (pinned `nomic-embed-text-v1.5-mlx` plus a dynamically managed local LLM supporting both `Qwen2.5-Coder-7B` and `mlx-community/Qwen3.6-35B-A3B-4bit` dynamically) provides the optimal balance between coding precision, VRAM footprint, and SSD longevity, allowing users to choose their model size while protecting the host with dynamic swap monitoring.

## Ambiguities
*   *Resolved:* How to handle external/cloud models under the new grammar constraints?
*   *Resolved:* How to prevent concurrent `cargo test` executions from locking the Cargo build directory?
    *   *Resolution:* Dynamically isolate `CARGO_TARGET_DIR` and run builds in offline mode (`--offline`) to bypass global registry lock contention.
*   *Resolved:* How to handle database corruption or binary format upgrades?
    *   *Resolution:* Implement Stale Database Recovery: automatically back up the corrupted database directory, initialize a fresh SurrealKV instance, re-index from Markdown files, and replay the episodic JSONL transaction log.

## Tradeoffs
*   **Performance vs. Portability (MLX vs. Cross-Platform):** Utilizing native `mlxrs`/`mlxcel` bindings provides maximum performance on Mac but locks compilation to macOS. We resolve this by using conditional compilation (`#[cfg(feature = "mlx")]`) and falling back to ONNX Runtime GenAI (`ort`) or external hosted APIs on Linux and Windows.
*   **Concretization vs. Flexibility (Local vs. Cloud):** Hosting the local LLM directly inside the daemon process eliminates IPC latency but increases local RAM usage. We resolve this by implementing a 2-Tiered Model Broker with dynamic VRAM swapping, while maintaining support for external hosted endpoints.
*   **RRF Scoring vs. Database Offloading:** Performing Reciprocal Rank Fusion (RRF) in the Rust daemon rather than natively in SurrealQL requires executing parallel queries and merging them in memory. This trade-off is accepted because it gives us maximum flexibility in tuning the RRF constant $k$ and ensures compatibility across different SurrealDB minor versions.

## Blocking Questions
None. All major design questions regarding model tiering, idle-gated scheduling, TDD tree integration, database persistence (SurrealKV), memory safety, and zero-friction CLI redirection have been resolved.

