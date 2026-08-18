---
labels: architecture-review, adversarial
state: open
---
# Red Team Architecture Brief: 18-Month Scaling Breakdown

**Finding** -> Three core architectural decisions will become severe technical debt when the system scales 10x.
**Current Assumption** -> The current local-first, monolithic design will scale linearly with hardware and agent complexity.
**Attack Scenario** -> Increased complexity leads to overlapping agent scopes, VRAM contention, and search degradation under enterprise/multi-agent workloads.
**Blast Radius** -> System grinds to a halt.
**Recommended Structural Change** -> Re-architect top 3 systems: 1) Extract Model Broker into a standalone GPU-scheduling cluster manager. 2) Replace hardcoded 6-Signal Unified Retrieval formula with an ML reranking model (Learning to Rank). 3) Migrate from single-tenant SurrealDB/RocksDB to a distributed vector database for cross-device orchestration. Must require an ADR response to close.
