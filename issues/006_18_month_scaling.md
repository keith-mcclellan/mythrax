---
title: "18-Month 10x Scale Bottlenecks: SQLite Cache, API Gateway, Markdown Vault"
labels: ["architecture-review", "adversarial"]
---

## Finding
Projecting the current architecture 18 months forward under a 10x scale scenario reveals three primary bottlenecks that will force re-architecture:
1.  **SQLite Embedding Cache (`embeddings.db`) I/O Locks:** Transaction-bounded batches and FIFO eviction will struggle under 10x concurrent writes from parallel agents, leading to severe lock contention.
2.  **Single-Port API Gateway Contention:** The shared `reqwest::Client` and unified router on Port 8090 will become a major bottleneck as MCP traffic, memory queries, and completions proxy calls saturate the port.
3.  **Streaming-to-Disk Cognitive Pipeline (Obsidian Vault):** Emitting thousands of synthesized artifacts and AST symbols incrementally as Markdown/JSON files will overwhelm the filesystem watcher and create massive I/O overhead.

## Current Assumption
The assumption is that SQLite's WAL mode can handle local persistence adequately, that a single port reduces orchestration complexity, and that flat Markdown files provide the best interoperability with human tools (Obsidian).

## Attack Scenario
While not strictly an external attack, a burst of high-concurrency agent activity (e.g., parallel repository analysis or large-scale code generation) acts as an inadvertent DoS. The SQLite cache locks up, the API Gateway drops requests due to pool exhaustion, and the OS file descriptor limit is hit by the Obsidian watcher, stalling the entire system.

## Blast Radius
**Medium to High.** Severe performance degradation and intermittent timeouts. As scale increases, the system transitions from responsive to sluggish, eventually failing to complete E2E cognitive loops.

## Recommended Structural Change
1. **Embedding Storage:** Migrate from SQLite to a dedicated, high-concurrency vector database (e.g., Qdrant or Milvus) or heavily shard the SQLite instances per project/agent.
2. **Gateway Refactoring:** Implement a robust API Gateway with connection pooling, rate limiting, and multi-port listening (e.g., separating control plane vs. data plane).
3. **Storage Engine:** Replace direct filesystem Markdown writes with a virtualized filesystem layer or an embedded graph database (like SurrealDB, which is already used, but pushing all artifacts there instead of disk), generating Markdown exports only on demand.