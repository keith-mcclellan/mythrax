---
title: Adversarial Review: 18-Month Projection - Top 3 Scaling Liabilities
labels: architecture-review, adversarial
---

**Finding:** Projecting the current architecture 18 months forward under a 10x scale scenario.
**Current Assumption:** The current design will linearly scale to handle 10x more agents, memory nodes, and concurrent tool invocations.
**Attack Scenario / Scaling Failure:**
1. **SurrealKV Local Engine Bottleneck:** Relying entirely on a single-node local `surrealkv://` instance. At 10x scale with multiple agents hammering the DB with concurrent read/writes, the local engine will suffer from lock contention and write-stalls, becoming the primary IO bottleneck. *Re-architecture: Distributed or client-server database architecture.*
2. **Synchronous AST Symbol Extraction in Ingestion Path:** Extracting AST symbols synchronously during ingestion will choke the pipeline when 10x more files are processed or when dealing with massive monorepos. *Re-architecture: Decouple AST extraction into a dedicated, scalable background worker queue.*
3. **Monolithic Tokio CancellationToken Shutdown:** Relying on a single shared `CancellationToken` for 5-second graceful shutdown. At 10x scale, complex VRAM flushes and DB transaction commits will frequently exceed 5 seconds, leading to forced SIGKILLs and corrupted states. *Re-architecture: Implement a staged, dependency-aware shutdown DAG with distinct timeouts per subsystem.*
**Blast Radius:** Severe performance degradation, unresponsiveness, and data corruption at scale.
**Recommended Structural Change:** Begin decoupling the storage layer, moving heavy processing to asynchronous queues, and implementing a staged shutdown manager.

*Note: Do not close this issue without a documented architectural decision record (ADR) response.*