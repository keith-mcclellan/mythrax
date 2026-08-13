---
title: "Finding: SQLite Embedding Cache FIFO Eviction based on Capacity"
labels: ["architecture-review", "adversarial"]
---

**Finding** -> SQLite Embedding Cache FIFO Eviction based on Capacity

**Current Assumption** -> Evicting the oldest entries (FIFO via `created_at`) when cache is full prevents OOM and keeps memory bounded.

**Attack Scenario** -> An adversarial agent intentionally feeds a stream of unique garbage embeddings to thrash the cache. This forces constant disk I/O to flush and evict legitimate embeddings, destroying retrieval performance.

**Blast Radius** -> Severe performance degradation. Search retrieval becomes slow and ineffective as useful embeddings are purged.

**Recommended Structural Change** -> Switch from FIFO to an LRU or LFU cache eviction policy, and implement quota limits per agent/scope to prevent cache poisoning and thrashing.
