---
title: "🛡️ Red Team Architecture Brief: 18-Month Scaling Risks & Re-Architecture Projection"
labels: ["architecture-review", "adversarial"]
status: "open"
---

# Red Team Architecture Brief

**Finding**
Projecting 18 months forward, three foundational decisions made today will become massive re-architecture projects if the system scales 10x.

**Current Assumption**
Local file-based locking, in-process DB coupling, and sequential VRAM eviction are sufficient for a single-user daemon.

**Attack Scenario / Failure Mode (At 10x Scale)**
1. **Local Exclusive File Locks (SurrealKV/RocksDB)**: The assumption that a 10-attempt, 500ms sleep loop handles contention will break under heavy concurrent multi-agent write pressure, causing constant timeouts and deadlocks.
2. **In-Process Metal GPU Engine**: Running models natively in the same Rust process as the database and API Gateway couples memory spaces. As models grow, OOM crashes during inference will take down the entire DB and Gateway simultaneously.
3. **Sequential VRAM Eviction**: "Evict unused models... wait for memory release" will cause severe "model thrashing" where concurrent agents constantly evict and reload each other's models, reducing system throughput to 0.

**Blast Radius**
Complete system gridlock, inability to deploy to cloud/cluster environments, and catastrophic memory crashes under load.

**Recommended Structural Change**
1. Transition the storage layer to an interface that supports distributed SQL/NoSQL backends (e.g., Postgres/Redis) rather than local file locks.
2. Extract the Model Broker into a standalone gRPC/REST inference microservice to isolate OOM crashes from the core DB and Gateway.
3. Implement a priority-queue based model scheduler with pipelined batching rather than naive sequential VRAM eviction.
