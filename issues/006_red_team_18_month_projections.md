---
tags: architecture-review, adversarial
---

# Red Team Architecture Brief: 18-Month Projections

**Finding**: The current architecture is heavily monolithic, relying on single-node data stores and coupled synchronous/asynchronous boundaries.

**Current Assumption**: The single-port gateway, SQLite embedding cache, and local SurrealKV engine will scale seamlessly with more agents and deeper memory contexts over time.

**Attack Scenario**: As load scales 10x, memory constraints, I/O bottlenecks, and single points of failure cascade, leading to system-wide lockups and complete unavailability under normal usage or targeted load generation.

**Blast Radius**: System failure at scale, requiring full rewrites of core components while production is unstable.

**Recommended Structural Change**:
The following top 3 decisions made today will become immediate re-architecture projects within 18 months if the system scales 10x:
1. **The Single-Port API Gateway (Monolith)**: Will need to be split into separate microservices for Control Plane (REST/MCP) and Data Plane (Model Proxy) to allow independent scaling and load shedding.
2. **Local SurrealKV Storage Engine**: The `surrealkv://` local engine is a single point of failure with no replication. It will need to be replaced with a distributed vector database or managed external store (e.g., Pinecone/Qdrant + Postgres).
3. **SQLite Embedding Cache with FIFO Eviction**: Time-based FIFO eviction will become a massive bottleneck, constantly thrashing critical memory. It must be replaced with a distributed cache (e.g., Redis) utilizing LRU/LFU or tiering mechanisms.
