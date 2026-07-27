---
title: "Red Team Architecture Brief: 18-Month Scaling Liabilities and Single Point of Failure"
labels: ["architecture-review", "adversarial"]
---

### Finding
Projecting the architecture 18 months forward, three major decisions will become critical re-architecture projects at 10x scale:
1. Local File DB Locks (RocksDB/SurrealKV)
2. Tightly-coupled In-Process GPU Inference
3. Single-Port API Gateway Design

### Current Assumption
The assumption is that Mythrax will operate primarily as a single-node, personal sidecar intelligence daemon where local file locking and single-process orchestration are sufficient.

### Attack Scenario / Scaling Failure
At 10x scale (e.g., enterprise deployment, multi-tenant agent orchestration, or massive concurrent memory ingestion):
1. **Local DB Locks**: SurrealKV/RocksDB will encounter severe contention, locking issues, and inability to scale horizontally. File I/O will become a massive bottleneck, preventing concurrent agent operations.
2. **Coupled GPU Inference**: Single-node VRAM will be permanently exhausted. The lack of distributed inference routing will cause deadlocks or OOM crashes, bringing down the entire daemon.
3. **Single-Port Gateway**: The gateway will fail under high connection loads. Mixing massive HTTP streams (proxy completions) with database read/writes on a single event loop will lead to socket starvation and latency collapse.

### Blast Radius
Complete systemic gridlock and inability to scale. The system will require a total rewrite to support distributed operations, breaking all existing assumptions about local state and deployment.

### Recommended Structural Change
1. **Pluggable Backend Storage**: Abstract the storage layer to support distributed databases (e.g., Postgres, Redis) alongside the local KV store.
2. **Microservice / Sidecar Decoupling**: Fully separate the Gateway, Storage Engine, and Inference Engine into independent, horizontally scalable services connected via robust message queues or gRPC.