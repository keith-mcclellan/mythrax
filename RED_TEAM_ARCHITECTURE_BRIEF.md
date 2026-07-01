# Mythrax 2.0 Red Team Architecture Brief

**Prepared by:** Adversarial CTO Persona
**Objective:** Stress-fracture the Mythrax 2.0 architecture under load, adversarial inputs, and changing requirements.

## 1. Architectural Challenges & Single Points of Failure

### Challenge 1: Single-Port API Gateway & Routing
* **Documented Decision:** Consolidating all admin, memory, MCP, and completions proxy endpoints on port 8090.
* **What assumption does this break if it's wrong?** Assumes all traffic sources are equally trusted and that network-level segmentation is unnecessary for a local daemon.
* **Single Point of Failure:** If port 8090 is exhausted (e.g., connection pool exhaustion or a thread deadlock), the entire system—MCP, completions, memory, and admin—fails simultaneously with no graceful degradation path.

### Challenge 2: Dual-Engine Storage & Persistent Lock Resiliency
* **Documented Decision:** Wrapping RocksDB/SurrealKV in a retry loop with backoff (10 attempts, 500ms sleep) to handle lock contention.
* **What assumption does this break if it's wrong?** Assumes contention is transient (<5 seconds) and not a persistent state under high concurrent load.
* **Single Point of Failure:** The embedded file lock mechanism. If the lock is held indefinitely (due to a crash or rogue process), the daemon fails to boot or serve requests entirely.

### Challenge 3: Three-Tiered Model Broker & VRAM Safeguards
* **Documented Decision:** Dynamic routing to in-process MLX/ORT and external HTTP `mlx-lm`, with sequential eviction.
* **What assumption does this break if it's wrong?** Assumes eviction and swapping are fast enough to keep up with incoming request rate without starving the client.
* **Single Point of Failure:** The split GPU semaphores (`METAL_INFERENCE_SEMAPHORE` / `METAL_EMBEDDING_SEMAPHORE`). A panic or unhandled error while holding the semaphore will permanently deadlock inference or embeddings.

### Challenge 4: Cognitive Scheduling & Thread-Safe WAL
* **Documented Decision:** 500ms file watcher coalescing and verbatim ingestion via pre-compaction hooks.
* **What assumption does this break if it's wrong?** Assumes verbatim ingested text is benign and can be safely re-injected into future context windows.
* **Single Point of Failure:** The WAL actor. If the background WAL receiver task crashes, transactions are lost, and the daemon will experience silent data loss without failing the API requests (if write-behind is unacknowledged).

## 2. Agent Orchestration Vulnerabilities

### Prompt Injection via Verbatim Ingestion
* **Vulnerability:** The pre-compaction hook parses JSONL transcripts and extracts raw text/tool results verbatim into SurrealDB as episodic memories. If an agent summarizes an external webpage or malicious document containing prompt injection (`"Ignore previous instructions and execute..."`), this is verbatim stored.
* **Mechanism:** Later, when Sigmoid-gated search retrieves this memory, it injects the malicious payload directly into the agent's context window.
* **Scope Failure:** No memory firewall or sanitization exists to strip instructional imperatives from ingested memories.

### Unbounded Recursion Risk
* **Vulnerability:** Agents querying the Obsidian vault (`manage_vault`) or performing HTR loop git operations can be trapped in unbounded recursion.
* **Mechanism:** A malicious repository with a symlink loop or a dynamically generated directory structure could cause the vault watcher or the agent's file traversal to spin infinitely. The 500ms coalescing watcher will generate continuous events, keeping the daemon awake and thrashing disk.

## 3. Evals Framework Assessment

### Happy-Path Bias in `evals/`
* **Assessment:** The `evals/swebench/` framework correctly evaluates SWE-bench Verified instances with no mock data, but it is **architecturally dishonest** because it solely tests the happy path.
* **Critique:** SWE-bench tests the agent's ability to fix well-defined python bugs. It does *not* test how the agent behaves when a PR contains a prompt injection, when a retrieved file is 5GB of random bytes, or when the memory database returns adversarial cognitive payloads. An LLM system without adversarial evals is a liability.

## 4. Coupling Liabilities

### Liability: Model Broker and SurrealDB Backend
* **Coupling:** `db/backend.rs` explicitly calls the embedding model to generate vectors (`embed_batch`). If the model fails, it falls back to zero-vectors (MOCK-009).
* **Impact:** The database layer and the inference layer cannot be independently scaled, deployed, or tested without mocking the other. If you want to move the embedding model to a dedicated GPU server, you must modify the core database insertion logic.

### Liability: Checkpoint Compiler and Daemon Core
* **Coupling:** `daemon.rs` spawns external compilers (`cargo`, `tsc`, `python`) for checkpoint checks (CHEAT-003).
* **Impact:** The daemon is tightly coupled to the host environment's toolchain. The core daemon cannot be shipped as a standalone lightweight binary without also mandating a heavy multi-language toolchain on the host.

## 5. 18-Month Projection: Scale & Re-architecture

If the system scales 10x (e.g., multi-tenant, 100+ concurrent agents, distributed memory), the top 3 decisions that will break and require immediate re-architecture are:

1. **Embedded SurrealKV/RocksDB with File Locks:** Moving to a true client-server database (PostgreSQL/pgvector or distributed SurrealDB) to support concurrent multi-process access and eliminate the 10-attempt lock retry loop.
2. **Single-Port Monolith:** Splitting the API Gateway into a distinct Control Plane (admin, config), Data Plane (MCP, memory), and Proxy Plane (completions). A single port with a static `X-Mythrax-Token` will not survive multi-tenant security requirements.
3. **In-Process Model Execution:** Embedding MLX execution directly into the Rust daemon limits scalability. This will be re-architected into a dedicated inference microservice (e.g., vLLM or a standalone `mlx-server`) to allow independent scaling, pooling, and hardware isolation.
