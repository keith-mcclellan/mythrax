---
labels: architecture-review, adversarial
---

# Red Team Architecture Brief: Tightly Coupled Storage Architecture (SurrealKV + SQLite)

**Note: This issue MUST NOT be closed without a documented Architectural Decision Record (ADR) response.**

**Finding** -> The architecture relies on a hybrid persistence model (SurrealKV for entities, SQLite for embedding cache), but the cognitive pipeline directly manipulates SurrealDB tables and SQLite caches without abstract repository interfaces.

**Current Assumption** -> SurrealKV and SQLite will scale linearly and indefinitely, and they will always run on the same local filesystem as the daemon.

**Attack Scenario** -> When scaling 10x (e.g., enterprise deployment across multiple nodes), the local SurrealKV engine becomes a bottleneck. Because the cognitive pipeline directly references SurrealDB tables (`pipeline_cluster`, `code_symbol`), replacing it with a distributed DB requires rewriting the entire pipeline.

**Blast Radius** -> Inability to scale horizontally. High architectural liability requiring a multi-month rewrite of the data access layer when limits are reached.

**Recommended Structural Change** -> Introduce a strict repository interface pattern. The cognitive pipeline must depend on traits (e.g., `VectorStore`, `MemoryStore`), abstracting away SurrealDB and SQLite specifics to allow independent deployment or replacement.
