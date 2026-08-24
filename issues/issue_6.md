# [Red Team] 18-Month Projection: Top 3 Architectural Liabilities at 10x Scale

**Labels:** architecture-review, adversarial

**Finding:** Projecting the architecture 18 months forward, three major decisions will become severe liabilities requiring complete re-architecture if the system scales 10x in context size, user base, or agent concurrency.

**Current Assumption:** The current single-node, tightly-coupled, sidecar architecture will scale linearly with hardware upgrades (e.g., more RAM, faster GPU).

**Attack Scenario / Failure Mode:**
1. **SurrealKV Local Embedded Engine:** At 10x scale, the volume of episodic memories, wiki nodes, and AST extractions will cause the embedded local `surrealkv://` engine to hit I/O bottlenecks. Multi-agent concurrent writes will saturate the local disk queue, and the inability to horizontally scale the database across nodes will strangle throughput.
2. **Synchronous Metal Semaphore Bottleneck:** The split `METAL_INFERENCE_SEMAPHORE` and `METAL_EMBEDDING_SEMAPHORE` assume a single local GPU. At 10x scale, agents will require distributed inference. The local semaphore design cannot coordinate across multiple GPUs or network nodes, leading to massive queueing delays.
3. **Monolithic Daemon Process:** The unified gateway and monolithic background loops (file watcher, compactor, router) running in a single process will exceed single-core execution limits for async scheduling. A slow block in the `compactor.rs` DBSCAN thread will begin starving the HTTP gateway's tokio workers, violating the non-blocking execution invariants.

**Blast Radius:**
- Catastrophic performance degradation, rendering real-time sidecar intelligence unusable due to multi-second latency for basic memory retrieval.

**Recommended Structural Change:**
1. **Distributed State:** Re-architect storage to support a decoupled, horizontally scalable vector and graph database backend (e.g., remote SurrealDB cluster or dedicated Qdrant/Neo4j cluster) rather than purely local embedded KV.
2. **Distributed Model Broker:** Abstract the Model Broker to a gRPC/network-based coordinator that can load-balance across a cluster of inference nodes rather than relying on local OS-level semaphores.
3. **Microservice Decomposition:** Split the daemon into separate micro-services: a dedicated Storage Engine, a Cognitive Compactor Worker, and an API Gateway, communicating via message queues.
- Require ADRs for all three projections prior to closing.