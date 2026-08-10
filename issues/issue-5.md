---
title: "Adversarial Architecture Review: 18-Month Scaling Bottlenecks (10x Scale)"
labels: architecture-review, adversarial
---

**Finding:** The current architecture will encounter severe bottlenecks and single points of failure if the system scales 10x over the next 18 months.
**Current Assumption:** Local single-node execution with 500ms file watchers and in-process synchronous pagination loops will scale indefinitely.
**Attack Scenario / Failure Mode:** Under 10x load, three critical decisions will fail:
1. **Local Persistent Storage Engine (SQLite/SurrealKV):** Will hit IOPS and lock-contention limits when multiple agents concurrently read/write high-density memories.
2. **In-Process Inference Execution:** Model inferences will block the tokio executor or cause VRAM fragmentation, crashing the main daemon.
3. **500ms File Watcher Coalescing:** With 10x files generated, the Obsidian vault watcher will thrash CPU and disk, triggering cascading LLM "dreaming" passes and exhausting the rate limits.
**Blast Radius:** Severe latency degradation, database lock starvation, and recurrent OOM daemon crashes, paralyzing the cognitive architecture.
**Recommended Structural Change:**
- Migrate state to a distributed, highly-available datastore (e.g., external PostgreSQL/pgvector).
- Offload all inference to dedicated, horizontally scalable worker nodes (e.g., vLLM or Triton clusters).
- Replace file system watchers with explicit event-driven commit logs (e.g., Kafka/Redis Streams) for memory synchronization.

Never close without a documented architectural decision record (ADR) response.
