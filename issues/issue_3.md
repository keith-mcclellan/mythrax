---
title: "Architecture Liability: SQLite Embedding Cache Eviction Attack"
labels: ["architecture-review", "adversarial"]
---

**Finding** -> The SQLite embedding cache uses FIFO eviction based on `created_at` timestamps when capacity is reached.

**Current Assumption** -> Embedding cache misses are cheap and cache thrashing is unlikely to occur in normal operations.

**Attack Scenario** -> An adversary intentionally generates a large volume of unique queries or context, forcing the cache to evict frequently used embeddings (thrashing). This causes constant re-computation of embeddings.

**Blast Radius** -> Severe performance degradation (DoS via resource exhaustion) as the Model Broker is forced to re-embed hot context continuously.

**Recommended Structural Change** -> Change the eviction strategy from FIFO to LRU (Least Recently Used) or LFU (Least Frequently Used) to protect hot context. Implement rate limiting on embedding generation.

*Note: Do not close this issue without a documented architectural decision record (ADR) response.*
