---
title: "18-Month Scaling Risks: 10x Load Re-architecture Requirements"
labels: ["architecture-review", "adversarial"]
status: "open"
---

## 🛑 Finding: Top 3 Scaling Risks for 10x Load (18-Month Projection)

**Finding:** The current architecture makes several decisions that optimize for single-user, local performance but will critically fail if the system scales 10x in throughput, concurrent agents, or data volume over the next 18 months.

**Current Assumption:** The system is designed for a single developer machine with manageable disk I/O, linear memory growth, and a predictable daily downtime for batch processing.

**Attack Scenario / Failure Mode:** Under 10x scale, these three specific architectural decisions will break:

1. **Single-Port Daemon (Port 8090):** Consolidating all REST, MCP, and proxy traffic onto a single port/process will hit file descriptor and connection limits. A burst of concurrent agent requests will saturate the Axum router, leading to dropped connections and timeouts.
2. **500ms File Watcher Coalescing:** The `notify` crate coalescing events over a 500ms window assumes low-frequency writes. At 10x scale, concurrent agent write cascades will either overwhelm the coalescing buffer (leading to dropped events) or cause infinite queuing if writes continuously extend the sliding window without committing.
3. **Daily DBSCAN Epsilon-Calibrated Compaction:** Running a batch DBSCAN clustering and RAPTOR tree generation process once daily works for small data. At 10x memory volume, the $O(n^2)$ or $O(n \log n)$ complexity of DBSCAN means this "dreaming" cycle will take longer than 24 hours to complete, meaning it will never catch up.

**Blast Radius:** The system becomes permanently bottlenecked, losing data consistency (dropped events) and suffering infinite processing lag (compaction never finishes).

**Recommended Structural Change:**
1. Shard the gateway layer and use a reverse proxy for connection management.
2. Replace the naive 500ms sliding window with an append-only event log (e.g., Kafka/Redpanda semantics) and asynchronous consumer groups.
3. Move away from batch daily DBSCAN to an online, incremental clustering algorithm that updates centroids in real-time as memories are ingested.
