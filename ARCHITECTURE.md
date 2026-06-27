# Mythrax 2.0 Architecture Reference

This document outlines the technical architecture, data flows, concurrency boundaries, and safety safeguards of **Mythrax 2.0**. The system is designed as a high-performance, secure, and self-healing sidecar intelligence daemon that acts as a unified memory, cognitive, and model routing server for autonomous AI agents.

```
                  +-------------------------------------------------+
                  |                  Agent/Client                   |
                  +-------------------------------------------------+
                                     |             |
                         REST / MCP  |             |  OpenAI API
                        (Port 8090)  |             |  (Port 8080/8090)
                                     v             v
                  +-------------------------------------------------+
                  |            Single-Port API Gateway              |
                  +-------------------------------------------------+
                                           |
                                           v
                  +-------------------------------------------------+
                  |             Mythrax 2.0 Core Daemon             |
                  +-------------------------------------------------+
                    |          |            |          |         |
                    v          v            v          v         v
             +----------+ +----------+ +--------+ +--------+ +-------+
             | Surreal  | |  Model   | |   FS   | | Thread | | Size  |
             |   KV /   | |  Broker  | | Watch  | |  Safe  | | Roll  |
             | RocksDB  | | (MLX/ORT)| | (500ms)| |  WAL   | | Logger|
             +----------+ +----------+ +--------+ +--------+ +-------+
```

---

## 1. Single-Port API Gateway & Routing

Mythrax 2.0 consolidates all administrative, memory, Model Context Protocol (MCP), and transparent completions proxy endpoints onto a unified, single-port gateway (**default port: 8090**).

- **Unified Router & Request Processing Flow**:
  - The Gateway binds to default port `8090`. It hosts the Axum REST router, mapping paths `/v1/episodes`, `/v1/config/llm`, `/v1/mcp/call`, and `/v1/chat/completions`.
  - **Authentication Boundary**: REST and MCP requests are validated against a shared static auth token via `X-Mythrax-Token` headers. Unauthorized requests fail with `401 Unauthorized`.
  - **Transparent Routing & Dispatching**: If the daemon port is active, all API/MCP operations are routed to the daemon (Client Mode). If inactive, the SDK falls back to Server Mode, opening the database and executing queries directly.
  - **Proxy Mode (Port 8080)**: Actively intercepts OpenAI-compliant completions requests, injecting status checks and formatting response streams dynamically.
- **Auto-Spawn Sequence**: Clients automatically detect if the daemon is running. If not, they spawn the background daemon process detached, verify its readiness via port polling for up to 15 seconds, and write the Process ID to `~/.mythrax/daemon.pid`.

---

## 2. Dual-Engine Storage & Persistent Lock Resiliency

To guarantee database integrity and solve concurrent process contention, Mythrax 2.0 implements a robust dual-engine storage model and connection retry mechanism.

- **SurrealKV & RocksDB Engines**: Supports both `surrealkv://` and `rocksdb://` local storage prefixes, ensuring all agent memories, handoffs, and cognitive graphs are fully persisted to disk.
- **Persistent Lock Retry Loop**: RocksDB and SurrealKV require exclusive file locks. In multi-process test runs or rapid daemon restarts, this often triggers lock contention errors. Mythrax 2.0 solves this by wrapping the database connection in a **retry loop with backoff** (up to 10 attempts, 500ms sleep) to wait for pending locks to release.
- **Startup Bootstrapping & WAL Replay Sequence**:
  1. **Port/Daemon Detection**: CLI detects running daemon port. If inactive, spawns detached daemon process and polls readiness.
  2. **Exclusive File Locking**: Database initializes via `SurrealBackend::new`. Reconnection retry attempts handle transient locks.
  3. **Schema Bootstrapping**: Runs schema definitions (`INIT_SCHEMA`) and inserts the default configuration `config:settings` (defaulting to `mlx-community/Qwen3.6-35B-A3B-4bit`).
  4. **Self-Healing WAL Replay**: Calls `replay_wal_if_fresh` to replay uncommitted transactions from the WAL log if the `.initialized` marker is absent or database is empty, then writes `.initialized` to prevent redundant replays.
  5. **Background WAL Actor**: Spawns a background WAL receiver task to write incoming transaction updates sequentially to disk with `0600` permissions.
- **Startup Pruning**: On startup, the daemon automatically runs background pruning loops to sweep stale handoffs, orphaned context links, and transient session files, keeping the database footprint compact.

---

## 3. Three-Tiered Model Broker & VRAM Safeguards

The cognitive and inference capabilities in Mythrax 2.0 are managed by a highly optimized, hardware-aware Model Broker.

- **Three-Tiered Engine**: Dynamic routing supports:
  1. **MLX (Local Apple Silicon)**: Exploits metal GPU acceleration for ultra-fast local inference and embeddings.
  2. **ORT (ONNX Runtime)**: Run-anywhere CPU/GPU ONNX model execution.
  3. **Mock Mode**: Light, in-memory simulations for lightning-fast testing and offline compilation.
- **Split GPU Semaphores**: To prevent deadlocks under heavy parallel workloads (e.g., when a background dreaming compaction runs while an agent is actively querying memory), the broker separates the pipelines into independent semaphores:
  - `METAL_INFERENCE_SEMAPHORE`: Coordinates model text generation.
  - `METAL_EMBEDDING_SEMAPHORE`: Coordinates vector embedding calculations.
- **VRAM Eviction & Sequential Swapping**: To run large models on consumer-grade hardware without Out-Of-Memory (OOM) crashes, the broker executes a sequential eviction loop. Before loading a new model into VRAM, it evicts unused models, flushes caches, and waits for memory release.

---

## 4. Cognitive Scheduling & Thread-Safe WAL

Mythrax 2.0 introduces advanced scheduling loops and transaction logging to guarantee durability and consistency.

- **500ms File Watcher Coalescing**: The Obsidian vault watcher utilizes the `notify` crate to detect file edits. To prevent high-frequency write cascades and ingestion races, events are coalesced over a **500ms sliding window** before being committed to the database.
- **Thread-Safe Write-Ahead Log (WAL)**: All database transactions and memory updates are journaled through a thread-safe WAL actor. In the event of an abrupt power loss or crash, the daemon replays transactions from the WAL using sequential replay markers to rebuild state.
- **DBSCAN Epsilon-Calibrated Compaction**: During the daily "dreaming" cycle, the compactor runs DBSCAN clustering on episodic memories. Epsilon parameters are dynamically calibrated to group related memories, which are then summarized via hierarchical RAPTOR trees into permanent `wiki_node` structures.
- **Pre-Compaction Hook & Verbatim Ingestion**: Before dreaming runs, the daemon executes a pre-compaction hook to ingest the active transcript of a session. The hook parses the session's JSONL transcripts line-by-line:
  - Supports flat string schemas (`role` and `content` as text strings).
  - Handles array-of-blocks schemas (e.g., `content` represented as an array of text/tool blocks) used by modern AI agent hosts like Claude Code and Gemini.
  - Extracts the raw text and tool results verbatim, indexing them into SurrealDB as episodic memories without dropping any tool output details.
- **Memory Co-existence & Retrieval Router (Flow 4)**:
  - **Co-existence Safeguard**: When episodic memories are summarized into permanent `wiki_node` structures via compaction, the original verbatim episodes are preserved in the database rather than replaced, allowing both high-level semantic retrieval and raw verbatim lookups to co-exist.
  - **Sigmoid Gating Formula**: Retrieval relevance scores are passed through a Sigmoid-gated filter to eliminate low-similarity matches:
    $$g = \frac{1}{1 + e^{-20(\text{similarity} - 0.60)}}$$
    This creates a soft step function centered at similarity `0.60` with a steepness of `-20.0`, clamping matches below `0.55` to near zero and boosting matches above `0.65`.
  - **Verbatim Floor / Decayed Episode Demotion**: Episodic memories that have decayed below a utility threshold (`utility < 10.0`) are marked as `archived = true` instead of deleted. Archived episodes remain retrievable but are heavily demoted in search ranking by multiplying their blended similarity score by a factor of `0.4` (a 60% demotion), keeping them visible as a baseline verbatim floor without cluttering top results.
- **Background Sweeps & Compaction Recovery (Flow 5)**:
  - **Idle Session Sweep**: The compaction daemon periodically scans registered session transcripts. If a session remains idle for $>10$ minutes, the compactor compares the file's last modified timestamp against the session's `_last_swept_at` record in Short-Term Memory (STM).
  - **Trailing Turn Ingestion**: If the transcript file contains un-ingested trailing turns, the compactor executes `mine_transcript` to parse and import them, then updates `_last_swept_at` to the current time.
  - **Orphan Cleanup**: If a registered transcript file has been deleted or is missing, the compactor purges the registered path from the STM registry to prevent polling loop leaks.

---

## 5. Thread-Safe Size-Rolling Logs & Graceful Shutdown

For production-grade operations, Mythrax 2.0 implements robust logging and clean lifecycle termination.

- **Thread-Safe SizeRollingFileWriter**: A custom thread-safe rolling writer writes logs to `~/.mythrax/daemon.log`. It automatically rolls the log file upon reaching **50MB** and maintains up to **3 historical backups** (`daemon.log.1`, `daemon.log.2`, `daemon.log.3`). Tracing is integrated via non-blocking guards to ensure no logs are lost on exit.
- **5-Second Graceful Shutdown Sequence**: Upon receiving a SIGINT (Ctrl+C) or SIGTERM signal, the daemon triggers a graceful shutdown sequence wrapped in a **5-second timeout**:
  1. Sleep for 500ms to allow active file-watcher events and database writes to finish.
  2. Evict all loaded models from VRAM via `broker.evict_unused_models()`.
  3. Clear Metal FFI caches and log the event.
  4. Flush and close the database connection.
  5. Delete the `daemon.pid` file and exit cleanly.
