---
title: "Finding 6: 18-Month Re-architecture Project - Hybrid Storage Engine Sprawl"
labels: ["architecture-review", "adversarial"]
---

**Finding**: The architecture uses a hybrid storage model: SurrealKV for agent memories/graphs and a SQLite Persistent Storage Engine for the Embedding Cache.

**Current Assumption**: The assumption is that splitting the storage engines provides the best of both worlds: SurrealKV's document/graph capabilities and SQLite's lightweight transactional caching, and that managing both simultaneously will remain low-overhead.

**Attack Scenario**: If the system scales 10x (e.g., massive concurrent agent sessions, huge context windows), managing cross-engine consistency becomes a nightmare. If a transaction fails in SurrealKV but succeeds in the SQLite embedding cache, the system enters a split-brain state where embeddings exist for non-existent memories, or vice versa. An adversary could trigger rapid, partial failures to purposefully desync the stores.

**Blast Radius**: Data corruption, semantic search failures (returning ghosts or missing critical context), and massive operational overhead trying to manually repair desynchronized databases.

**Recommended Structural Change**: This will be a major re-architecture project in 18 months. Plan to consolidate onto a single storage substrate that natively supports both vector search/embeddings and complex graph/document relations, or implement a robust, unified transaction coordinator (Two-Phase Commit) that guarantees atomicity across both SurrealKV and SQLite.
