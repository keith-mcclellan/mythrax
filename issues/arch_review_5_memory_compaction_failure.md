---
labels: architecture-review, adversarial
---

# Red Team Architecture Brief: Compaction Loop Failure Graceful Degradation is Missing

**Note: This issue MUST NOT be closed without a documented Architectural Decision Record (ADR) response.**

**Finding** -> The Daily dreaming compactor and streaming DBSCAN clustering pipelines are critical for long-term memory. If they fail (e.g., due to OOM on a massive cluster or VRAM exhaustion during hierarchical RAPTOR synthesis), there is no fallback.

**Current Assumption** -> The async cancellation token and memory limits (50 items) will always prevent failure, and incomplete clusters can be safely purged.

**Attack Scenario** -> A user rapidly ingests massive amounts of dense, highly related text. The clustering algorithm hits an edge case, constantly timing out or OOMing. The compaction loop continuously fails, causing the `episode` table to grow unbounded without summarization.

**Blast Radius** -> Complete degradation of the agent's long-term recall and eventual local disk exhaustion due to uncompacted raw episodic turns.

**Recommended Structural Change** -> Implement a fallback compaction strategy (e.g., simple sliding window summarization or heuristic-based eviction) when semantic clustering fails repeatedly. Add circuit breakers to pause ingestion if the compaction backlog exceeds a critical threshold.
