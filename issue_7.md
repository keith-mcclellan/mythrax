---
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief: 18-Month 10x Scale Projection

**Finding:** The current architecture is designed for a single-node, single-user local deployment. When projecting 18 months forward and anticipating a 10x scale in usage, memory corpus size, and concurrent agents, three specific design decisions made today will become immediate re-architecture liabilities:

1. **Embedded Database vs Distributed Storage:** Relying on SurrealKV/RocksDB embedded file-locks will completely break down. As the memory corpus grows and multiple high-throughput agents attempt concurrent retrieval and verbatim ingestion, disk IO and file lock contention will cause system-wide blocking. This will require a rip-and-replace migration to a horizontally scalable, standalone vector database cluster (e.g., Qdrant, Milvus).
2. **In-Process Model Broker vs Dedicated Inference Endpoints:** Loading dense models into the same Rust process memory via Metal FFI ties the control plane directly to the GPU's fate. At 10x scale, inference queues will overwhelm the daemon. This will force a split: moving all model execution to a dedicated VLLM/TGI inference cluster, completely deprecating the internal model broker.
3. **Synchronous 500ms File Watcher Coalescing:** The 500ms sliding window for the Obsidian vault watcher works for local human typing. At scale, bulk automated edits across thousands of nodes will overwhelm the coalescing window, triggering cascading re-ingestions and DB write cascades. This will require migrating from a reactive file watcher to an event-driven Kafka/RabbitMQ ingestion pipeline with robust backpressure.

**Current Assumption:** The system will remain constrained to a single user's workstation with bounded hardware and sequential memory workflows.

**Attack Scenario:** A 10x increase in concurrent agent activity or an expansion to a multi-tenant environment organically triggers a Denial of Service. The file lock queue saturates, the in-process Metal FFI panics due to out-of-memory errors, and the file watcher loops endlessly attempting to ingest a massive vault delta.

**Blast Radius:** Complete architectural stall. The team will be forced to halt all feature development for 3-6 months to extract the database, decouple the inference engine, and build a real event-streaming ingest pipeline.

**Recommended Structural Change:** Implement strict modular boundaries now via gRPC/Protobuf interfaces for Storage, Inference, and Ingestion. Even if they run locally today, defining network-ready interfaces prevents deep coupling and makes the future extraction to standalone microservices significantly cheaper. Require a mandatory ADR response to close this issue.