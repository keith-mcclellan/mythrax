---
labels: ["architecture-review", "adversarial"]
---
# Red Team Architecture Brief

## 1. Single-Port API Gateway & Shared Static Token Auth
* **Finding:** The Single-Port API Gateway operates on a single port (8090) and relies on a "shared static auth token via `X-Mythrax-Token` and `Authorization` headers."
* **Current Assumption:** Consolidating all endpoints (REST, MCP, transparent proxy) behind one port with a single static token simplifies deployment and is sufficient for local daemon isolation.
* **Attack Scenario:** An attacker or malicious local process exfiltrates the static token (which is shared and likely stored in plaintext). Because all administrative, memory, and proxy endpoints share this single point of failure and port, the attacker immediately gains full root-level control over the daemon, including memory modification (prompt injection into the db) and remote model execution. Additionally, sharing a single `reqwest::Client` creates contention, causing a Denial of Service under load.
* **Blast Radius:** Complete system compromise. A leaked token leads to full data corruption (R/W to SurrealDB/SQLite), arbitrary cognitive execution, and DoS via connection exhaustion on the single port.
* **Recommended Structural Change:** Implement a dual-port architecture separating unprivileged completions/proxy traffic from privileged administrative/MCP control planes. Replace the static token with dynamic, short-lived scoped tokens specific to each client and port. Eliminate the shared `reqwest::Client` in favor of a connection pool.

## 2. In-Process Engine native MLX Model Loading
* **Finding:** The daemon loads MLX models natively into the process memory using the MLX Metal GPU backend.
* **Current Assumption:** Loading models directly into the daemon's process space minimizes latency and simplifies state management.
* **Attack Scenario:** An adversarial input or edge case causes the MLX C++ bindings to OOM or panic. Because the engine is in-process, this abruptly crashes the entire daemon.
* **Blast Radius:** Complete loss of availability. Any inflight writes to SQLite or SurrealKV could be corrupted, and the daemon requires a full restart, disrupting all connected agents.
* **Recommended Structural Change:** Isolate model execution into a separate sidecar process (Model Execution Node) that communicates with the Core Daemon via IPC/RPC. The Core Daemon should supervise the sidecar and seamlessly restart it on crash without dropping client connections or corrupting the database.

## 3. SQLite Embedding Cache (embeddings.db) I/O Locks
* **Finding:** Mythrax uses a SQLite persistent store (`embeddings.db`) for its embedding cache.
* **Current Assumption:** SQLite can handle the concurrent read/write load of the embedding cache in a multi-agent scenario.
* **Attack Scenario:** Under heavy concurrent load (e.g., multiple agents scaling 10x and concurrently writing memory embeddings), SQLite encounters database lock contention. An adversarial workload could spam cache misses, causing a write storm that locks the database.
* **Blast Radius:** System bottleneck and timeout cascades. All agent memory ingestion pipelines halt waiting for SQLite locks, causing the gateway to timeout and drop requests.
* **Recommended Structural Change:** Migrate the embedding cache to a high-concurrency embedded key-value store (like RocksDB or Sled) or implement a dedicated write-ahead logging (WAL) asynchronous queue that serializes writes to SQLite while serving reads from an in-memory structure.

## 4. Unbounded Temporal Expansion & Sliding Window LIMITs
* **Finding:** Temporal expansion graph traversals apply `LIMIT 50` constraints per hop level.
* **Current Assumption:** Capping traversals to 50 items per level prevents graph explosions.
* **Attack Scenario:** A malicious agent constructs a highly dense memory cluster containing deeply nested references. A depth-3 traversal with `LIMIT 50` yields up to 125,000 nodes (50^3). This causes unbounded recursion/traversal risks and massive memory spikes, resulting in a denial of service.
* **Blast Radius:** Denial of Service. The daemon exhausts memory or processing time building the graph, starving other requests and potentially crashing the process.
* **Recommended Structural Change:** Implement an absolute global limit on the number of nodes visited during graph traversal (e.g., max 1000 nodes total, regardless of depth). Introduce traversal budget decay and cycle detection to prevent adversarial dense-graph DoS.

## 5. Agent Orchestration Prompt Injection in Episodic Memory
* **Finding:** External inputs (user inputs, API tool results) are stored in prompt-visible episodic memory hooks (like `precompact.rs`).
* **Current Assumption:** The LLM can safely differentiate between system instructions and episodic memory content.
* **Attack Scenario:** An attacker injects control tokens (like `<|`, `|>`) or markdown backticks into an API response. When the orchestrator retrieves this episodic memory, the LLM misinterprets the data as new instructions, hijacking the agent's behavior.
* **Blast Radius:** Complete agent hijacking. The agent can be manipulated to execute unauthorized actions, exfiltrate data, or corrupt memory boundaries.
* **Recommended Structural Change:** Implement strict sanitization of all external inputs before storing them in memory (e.g., escaping control tokens). Enforce structural separation in the prompt (e.g., using specific roles or structured data formats that the LLM is trained to ignore as instructions) and validate output against strict schema boundaries.

## 6. Eval Framework Happy-Path Bias
* **Finding:** The evaluation framework in `evals/swebench/` solely focuses on verified, functional 'happy paths' (SWE-bench coding benchmarks).
* **Current Assumption:** Passing standard coding benchmarks equates to production readiness and safety.
* **Attack Scenario:** The system is deployed into an adversarial environment where inputs are maliciously crafted to trigger OOMs, prompt injections, or race conditions. Because these were never tested, the system fails catastrophically in production.
* **Blast Radius:** High vulnerability exposure. The system may appear robust but is fragile against real-world adversarial usage, leading to silent failures, crashes, or compromises.
* **Recommended Structural Change:** Expand the evaluation framework to include an adversarial test suite. This should involve fuzzing API endpoints, testing prompt injection payloads, testing malformed SQLite data, and simulating high-concurrency race conditions.

## 7. Coupling of Streaming-to-Disk Cognitive Pipeline and Obsidian Vault
* **Finding:** The Streaming-to-Disk Cognitive Pipeline writes directly to Obsidian Vault markdown files.
* **Current Assumption:** Tying the cognitive output format to Obsidian markdown is sufficient and won't hinder future scaling or storage changes.
* **Attack Scenario:** If the system needs to scale to distributed storage (S3) or support a different consumer format, the tight coupling requires rewriting the core cognitive pipeline. Furthermore, disk I/O bottlenecks during streaming writes can stall the pipeline.
* **Blast Radius:** Architectural rigidity and potential I/O bottlenecks. Inability to easily replace the storage backend without modifying the cognitive synthesis logic.
* **Recommended Structural Change:** Introduce an abstract `StorageProvider` interface. The cognitive pipeline should emit structured events or domain objects, and a separate storage adapter should handle formatting (to Markdown, JSON, etc.) and persistence (Disk, S3, etc.).

## Future Projection (18 Months / 10x Scale) Re-architecture Projects
If the system scales 10x in 18 months, the following decisions will become major re-architecture projects:
1. **Single-Port API Gateway & Shared Auth:** Will require a distributed API gateway with granular RBAC and connection pooling to handle concurrent multi-agent traffic.
2. **In-Process MLX Model Loading:** Will need to transition to a distributed model-serving cluster (like vLLM or TGI) communicating via gRPC, as single-node in-process execution cannot scale horizontally.
3. **SQLite Embedding Cache & Direct Disk Writes:** Will demand a migration to a distributed vector database (e.g., Milvus, Qdrant) and blob storage (S3) to decouple compute from local state and support high-throughput vector search.
