# Mythrax 3.0 Architecture Reference

This document outlines the technical architecture, data flows, concurrency boundaries, and safety safeguards of **Mythrax 3.0**. The system is designed as a high-performance, secure, and self-healing sidecar intelligence daemon that acts as a unified memory, cognitive, and model routing server for autonomous AI agents.

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
                  |             Mythrax 3.0 Core Daemon             |
                  +-------------------------------------------------+
                    |          |            |          |         |
                    v          v            v          v         v
             +----------+ +----------+ +--------+ +--------+ +-------+
             | Surreal  | |  Model   | |   FS   | |  Arbor | | Size  |
             |  KV /    | |  Broker  | |  Watch | |  HTR   | | Roll  |
             |  SQLite  | | (MLX/ORT)| | (500ms)| |  Loops | | Logger|
             +----------+ +----------+ +--------+ +--------+ +-------+
```

---

## 1. Single-Port API Gateway & Routing

Mythrax 3.0 consolidates all administrative, memory, Model Context Protocol (MCP), and transparent completions proxy endpoints onto a unified, single-port gateway (**default port: 8090**).

- **Unified Router & Request Processing Flow**:
  - The Gateway binds to default port `8090`. It hosts the Axum REST router, mapping paths `/v1/episodes`, `/v1/config/llm`, `/v1/mcp/call`, and `/v1/chat/completions`.
  - **Authentication Boundary**: REST and MCP requests are validated against a shared static auth token via `X-Mythrax-Token` headers. Unauthorized requests fail with `401 Unauthorized`.
  - **Transparent Routing & Dispatching**: If the daemon port is active, all API/MCP operations are routed to the daemon (Client Mode). If inactive, the SDK falls back to Server Mode, opening the database and executing queries directly.
  - **Proxy Mode (Port 8080)**: Actively intercepts OpenAI-compliant completions requests, injecting status checks and formatting response streams dynamically.
- **Auto-Spawn Sequence**: Clients automatically detect if the daemon is running. If not, they spawn the background daemon process detached, verify its readiness via port polling for up to 15 seconds, and write the Process ID to `~/.mythrax/daemon.pid`.

---

## 2. Storage Architecture & Persistent Storage Engine

To guarantee database integrity, high performance, and resolve memory bottlenecks under large context ingestion, Mythrax 3.0 implements a hybrid SurrealKV and SQLite persistent storage architecture:

- **SurrealKV Storage Engine**: Operates exclusively on `surrealkv://` local key-value engine for SurrealDB, ensuring all agent memories, wiki nodes, directions, handoffs, and cognitive graphs are safely persisted to disk without third-party native library dependencies.
- **SQLite Embedding Cache**: Replaces legacy monolithic binary embedding cache serialization (`embedding_cache.bin`) with a lightweight SQLite persistent store (`embeddings.db`). Cache writes and flushes operate incrementally via transaction-bounded batches, backed by FIFO eviction on maximum capacity constraints (using `created_at` timestamps) to prevent startup OOM spikes.
- **Incremental IDF Indexer (`idf_index` Table)**: Eliminates bulk-loading episode content into memory during search score calculations. Document term frequencies are tracked incrementally in the `idf_index` SurrealDB table (keyed by `term` and `scope`), while total document counts are computed dynamically via scope queries (`SELECT count() FROM episode WHERE scope = $scope`).
- **Hash-Based Deduplication (`content_hash` Field)**: Episodes, wisdom rules, and wiki nodes store a SHA-256 hash of normalized content (`content_hash`) with indexed database lookups (`idx_content_hash`, `idx_wiki_node_hash`) to provide O(1) duplicate detection via `find_duplicate_by_content_hash` and `find_wiki_node_by_hash` without in-memory string comparison sweeps.
- **Ephemeral Pipeline DBSCAN State (`pipeline_cluster` Table)**: Ephemeral DBSCAN clustering assignments during cognitive synthesis are stored in SurrealDB temporary table `pipeline_cluster` (keyed by `run_id`, `cluster_id`, `episode_id`), guaranteeing transactional state safety without file-system watcher re-ingestion races. Cleanup (`delete_pipeline_run`) is enforced via RAII scope guards (`scopeguard::defer!`).
- **Workspace & Project Documentation Vault Mirroring (`sync_workspace_docs_to_vault`)**:
  - Scans workspace documentation (`ARCHITECTURE.md`, `REINITIALIZATION.md`, `conductor/tracks/**/*.md`, `specs/**/*.md`) and mirrors them into `vault_root/reference/` with relative directory structure preserved.
  - Implements SHA-256 content hash comparison to skip unchanged files without disk writes or re-indexing overhead.
  - Performs surgical vault and DB cleanup via `delete_by_vault_path` (`delete_by_vault_path_db`), purging deleted files, `WikiNode` records, and associated graph edges (`relates_to` / `followed_by`).
  - Suppresses watcher loop events on `reference/`, `MOC.md`, and `*.tmp` paths to prevent cascading LLM dreaming passes.
  - Atomically updates `MOC.md` reference index header via `.tmp` swap without modifying user-curated MOC sections.
  - Indexes reference doc chunks with `node_type: "reference"` and `scope: "workspace_ref"`.
- **Persistent Lock Retry Loop**: File locks are protected during multi-process execution or daemon restarts via connection retries with exponential backoff (up to 10 attempts, 500ms sleep).
- **Startup Bootstrapping & Transaction Initialization Sequence**:
  1. **Port/Daemon Detection**: CLI detects running daemon port. If inactive, spawns detached daemon process and polls readiness.
  2. **Exclusive File Locking**: Database initializes via `SurrealBackend::new`. Reconnection retry attempts handle transient locks.
  3. **Schema Bootstrapping**: Runs schema definitions (`INIT_SCHEMA`), including `idf_index`, `pipeline_cluster`, `content_hash` indices, and purges orphaned `pipeline_cluster` records from prior aborted runs. Inserts default configuration `config:settings`.
  4. **Transaction-Aware Ingestion**: Leverages SurrealDB `BEGIN TRANSACTION` and `COMMIT TRANSACTION` boundaries for safe, atomic batch insertions.
  5. **Atomic File Operations**: Writes temporary candidate files to disk and renames them atomically to target destinations, preventing data corruption on abrupt termination.

---

## 3. Three-Tiered Model Broker & VRAM Safeguards

The cognitive and inference capabilities in Mythrax 3.0 are managed by a hardware-aware Model Broker enforcing strict evaluation and VRAM memory boundaries:

- **Three-Tiered Engine**: Dynamic routing supports:
  1. **MLX (Local Apple Silicon)**: Exploits metal GPU acceleration for ultra-fast local inference and embeddings.
  2. **ORT (ONNX Runtime)**: Run-anywhere CPU/GPU ONNX model execution.
  3. **Mock Mode**: Light, in-memory simulations for fast testing and offline compilation.
- **MLX Lazy Graph Evaluation Invariants (`.eval()`)**:
  - To prevent catastrophic Metal GPU unified memory leaks from unevaluated computational graphs, MLX array operations MUST be explicitly evaluated before holding references or extracting raw buffers:
    - **KV Cache Concatenation**: Concatenated key/value arrays in attention blocks (`Qwen2Attention::forward`) must execute `.eval()?` immediately after concatenation.
    - **Weight Dtype Casts**: Model weight arrays cast during loading (`as_dtype`) must execute `.eval().unwrap()` before being inserted into weight maps.
    - **Cross-Encoder Logit Access**: Logit tensors in cross-encoders must execute joint evaluation (`mlx_rs::eval(&[&logit_0, &logit_1])?`) prior to calling `.as_slice()`.
- **VRAM Tracking State Synchronization & Eviction**:
  - `acquire_llm` updates `active_tier` and `last_weak_ref` on cache hit to prevent tracking state desynchronization.
  - Idle VRAM eviction (`broker.evict_unused_models()`) drops unused model weights prior to loading higher-tier models or entering idle state.
- **Connection Pooling & Socket Reuse (`reqwest::Client`)**:
  - Reuses a shared HTTP client (`reqwest::Client`) across API endpoints, MCP tool invocations, and external completions proxy calls, eliminating TCP socket exhaustion and OS file descriptor leaks under high request volumes.
- **Hybrid In-Process vs External Routing**:
  - **In-Process Engine**: Lightweight dense models (e.g., Nomic embeddings and the Qwen2.5-0.5B/1.5B/7B family) load natively into process memory and run in-process using the Metal GPU backend.
  - **External Model Delegation**: Large hybrid models (such as `mlx-community/Qwen3.6-35B-A3B-4bit`) route directly to local `mlx-lm` HTTP completions server on port 8080.
- **Split GPU Semaphores**:
  - `METAL_INFERENCE_SEMAPHORE`: Coordinates model text generation.
  - `METAL_EMBEDDING_SEMAPHORE`: Coordinates vector embedding calculations.
- **VRAM Eviction & Sequential Swapping**: Before loading a new model into VRAM, the broker evicts unused models, flushes caches, and waits for memory release.

---

## 4. Cognitive Scheduling & Streaming Pipeline Architecture

Mythrax 3.0 decouples cognitive synthesis and memory compaction into a streaming-to-disk architecture backed by strict pagination and sliding window caps:

- **Streaming-to-Disk Cognitive Pipeline**:
  - Human-readable cognitive artifacts (episodes, summaries, insights, directions, wisdom rules, compactions) are written incrementally to Obsidian Vault markdown files (`vault/episodes/*.md`, `wiki/<scope>/*.md`, `wisdom/*.md`) as they are generated. Intermediate objects are dropped immediately rather than held in memory buffers.
  - Ephemeral machine state (DBSCAN clusters) is stored temporarily in SurrealDB `pipeline_cluster` tables and cleaned up upon pipeline conclusion.
- **Bounded Pagination & Query Constraints**:
  - All database reads avoid unbounded `get_all_*` calls, replacing them with bounded pagination (`LIMIT 50`) loops or streaming cursors.
  - HTTP handlers stream paginated records directly into chunked JSON response streams rather than accumulating full result sets in memory.
  - Temporal expansion graph traversals apply `LIMIT 50` constraints per hop level (depth 1/2/3) to prevent graph explosion on dense memory clusters.
  - Prompt concatenation for cluster insight synthesis enforces a strict 32K token budget limit, stopping database member fetches as soon as the budget is reached.
- **Sliding Window Caps & Batch Stream Bounds**:
  - **Transcript Tool Sequence Cap**: Tool mining in `precompact.rs` uses a 1,000-element `VecDeque` sliding window cap, discarding older events to bound memory usage during long session analysis.
  - **Forge Document Chunking**: `forge.rs` ingests documents in bounded chunk batches (5 items per batch), executing concept/rule extractions and parallel embeddings without accumulating full document trees.
  - **API Batch Size Limits**: `save_episodes_batch_handler` enforces payload size limits to reject oversized requests.
  - **Completions Proxy Truncation**: Prompt history in `completions_proxy_handler` is bounded by max token limits to prevent unbounded string allocations.
- **500ms File Watcher Coalescing**: Obsidian vault watcher coalesces file edit events over a 500ms sliding window.
- **Arbor HTR Parallel Verification Loop**: Evaluates candidate changes within isolated git worktrees using distinct target folders and ports.
- **DBSCAN Epsilon-Calibrated Compaction**: Daily dreaming compactor clusters episodic memories via dynamic epsilon calibration, writing hierarchical RAPTOR summaries to vault markdown files.
- **Verbatim Ingestion & Sigmoid Gated Search**:
  - Verbatim episodic memories are preserved alongside compact summaries.
  - Search ranking passes similarity scores through a Sigmoid-gated filter ($g = \frac{1}{1 + e^{-20(\text{similarity} - 0.60)}}$) and applies a $0.4$ demotion factor to archived records ($utility < 10.0$).

---

## 5. Async Runtime Safety & Graceful Shutdown

Mythrax 3.0 provides robust thread safety, async task cancellation, and signal termination:

- **Async Semaphore Models & Non-Blocking Spin-Loops**:
  - Replaces blocking spin-loops (`std::thread::sleep` + `try_acquire`) in async routines with non-blocking `tokio::sync::Semaphore::acquire().await` to yield control back to the tokio runtime executor.
  - Synchronous legacy functions adapt via Strangler pattern (`*_async` variants) or `tokio::task::block_in_place` bridges when async bubbling is structurally blocked by sync traits, preventing runtime panics.
- **Background Task Lifecycle & `CancellationToken`**:
  - All background loops (daemon timers, watcher tasks, session sweeps, compaction schedulers) register a shared `tokio_util::sync::CancellationToken`.
  - Loops evaluate `tokio::select! { _ = token.cancelled() => break, ... }` to guarantee complete shutdown within 5 seconds of signal receipt.
  - Temporary DBSCAN pipeline state cleanup (`delete_pipeline_run`) is guarded by RAII scope guards (`scopeguard::defer!`) to guarantee execution on early returns, `Err(?)`, or panics.
- **Thread-Safe Size-Rolling Logs**: Custom thread-safe writer rolls `~/.mythrax/daemon.log` at **50MB** and maintains **3 backups**.
- **5-Second Graceful Shutdown Sequence**:
  1. Trigger shared `CancellationToken` to halt background loops.
  2. Sleep for 500ms for active IO and DB writes to complete.
  3. Evict loaded VRAM models via `broker.evict_unused_models()`.
  4. Flush and close database connection.
  5. Delete `daemon.pid` file and exit cleanly.

---

## 6. End-to-End Cognitive Memory Data Flow

The following data flow trace summarizes the path of session telemetry, streaming compaction, and model execution:

```
[Agent Action / Chat Turn]
           │
           ▼
[Pre-Invocation Hook] ──► Extracts text & tool output verbatim (JSONL array)
           │
           ▼
[SurrealDB Episode Ingestion] ──► SHA-256 content_hash deduplication check
           │
           ├──► [Obsidian Vault Incremental Writer] ──► Writes vault/episodes/*.md
           │
           ▼ (Idle Session > 10m Sweep / Bounded Pagination LIMIT 50)
[Compactor Sweep Service]
           │
           ├──► [Model Router]
           │         │
           │         ├──► Small Dense (0.5B) ──► Loads In-Process (Metal GPU + .eval())
           │         │
           │         └──► MoE Hybrid (35B)  ──► Delegates to external HTTP (:8080)
           │
           ▼ (Streaming DBSCAN Clustering & RAPTOR Synthesis)
[Sigmoid & IDF Indexer] (Pre-computed idf_index lookup without content load)
           │
           ├──► [SurrealDB pipeline_cluster Table] ──► Ephemeral cluster state (RAII cleanup)
           │
           ├──► [Incremental Vault Flusher] ──► Streams wiki/<scope>/*.md & wisdom/*.md
           │
           └──► [Arbor HTR Verification] ──► Runs verification in git worktree branches
           │
           ▼
[Knowledge WikiNode / Wisdom Rule Complete] (Vault md persisted, DB indexed)
```

---

## 7. Memory Safety Invariants

Mythrax 3.0 enforces mandatory memory safety invariants across all subsystems to guarantee sub-GB RAM footprint stability during high-throughput workloads:

1. **Pipeline Stage Memory Cap**: No cognitive pipeline stage, vector search expansion, or batch ingestion loop may hold more than **50 intermediate items** in memory simultaneously.
2. **No Unbounded Queries**: Calling `get_all_episodes()`, `get_all_wiki_nodes()`, or `get_all_wisdom_rules()` without pagination bounds is strictly forbidden. All database queries must use paginated loops (`LIMIT 50`) or streaming cursor iterators.
3. **HTTP Streaming Serialization**: HTTP batch endpoints and query handlers MUST stream paginated results via chunked JSON serialization directly from database cursors rather than materializing full result vectors in memory.
4. **Mandatory MLX Graph Evaluation**: All MLX array concatenations, weight dtype casts, and cross-encoder logit extractions MUST execute `.eval()` immediately before buffer access or storage.
5. **O(1) Hash-Based Deduplication**: Content duplicate checks MUST query the indexed `content_hash` field (`idx_content_hash`, `idx_wiki_node_hash`) instead of performing full-text content comparisons or in-memory string matching sweeps.
6. **O(1) Pre-Computed IDF Lookups**: FTS relevance scoring MUST query term frequencies from `idf_index` table (`idx_idf_term`), calculating total document counts dynamically without loading raw episode content fields into RAM.
7. **Ephemeral State Isolation & Guaranteed Purge**: Temporary machine state (DBSCAN clusters) MUST use database tables rather than in-memory arrays and MUST be wrapped in RAII scope guards to guarantee deletion upon completion or failure.
8. **Asynchronous Non-Blocking Execution**: Long-running or IO-bound routines inside the tokio runtime MUST NOT execute blocking thread sleeps (`std::thread::sleep`) or blocking lock spin-loops (`try_acquire` loops). They MUST yield to the executor using async primitives (`tokio::time::sleep`, `tokio::sync::Semaphore::acquire().await`) or `tokio::task::block_in_place`.
9. **Sliding Window & Truncation Bounds**: Mining sequences, transcript history, and API batch payloads MUST enforce strict window size limits (e.g., 1,000 elements for tool sequences, token budget bounds for chat proxies) to prevent unbounded memory growth.
10. **HTTP Client Socket Reuse**: All HTTP and external API communication MUST reuse shared `reqwest::Client` instances to avoid socket leaks and file descriptor exhaustion under load.
11. **Workspace Vault Mirroring Isolation**: Workspace doc sync (`sync_workspace_docs_to_vault`) MUST ignore build/VCS paths (`target/`, `.git/`, `.venv/`, `node_modules/`), check SHA-256 hashes before disk writes, ignore vault reference paths in file watchers, and execute atomic `.tmp` swaps for `MOC.md`.
