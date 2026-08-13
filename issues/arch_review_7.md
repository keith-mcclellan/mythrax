---
title: "Finding: 18-Month Re-architecture Liability - Monolithic SurrealDB + RocksDB Engine"
labels: ["architecture-review", "adversarial"]
---

**Finding** -> 18-Month Re-architecture Liability - Monolithic SurrealDB + RocksDB Engine

**Current Assumption** -> A single local SurrealDB instance using RocksDB/SurrealKV handles all temporal graphs, vector embeddings, and STM.

**Attack Scenario** -> At 10x scale, the single local database becomes an I/O and lock contention bottleneck. Graph traversals, vector searches, and high-frequency STM writes will saturate the local disk.

**Blast Radius** -> Unrecoverable performance degradation at scale.

**Recommended Structural Change** -> Split the storage architecture: Vector search to a dedicated vector DB (e.g., Qdrant/Milvus), STM to Redis/Memcached, and graph/relational data to PostgreSQL.
