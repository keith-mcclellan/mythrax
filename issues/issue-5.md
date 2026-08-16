# Top 3 Architectural Liabilities for 10x Scale in 18 months

**Tags:** `architecture-review`, `adversarial`
**Note:** Never close this issue without a documented architectural decision record (ADR) response.

**Finding** -> Three major architectural decisions will fail under 10x scale requirements within 18 months
**Current Assumption** -> The single-node SurrealKV/RocksDB architecture, the hardcoded 32K token budget limit for DBSCAN clusters, and the 500ms Obsidian file watcher coalescing window will scale linearly.
**Attack Scenario** -> As the system hits 10x load: 1) The single RocksDB node encounters severe write lock contention. 2) Large enterprise codebases consistently exceed the 32K token limit, causing insight synthesis truncation and lost data. 3) High concurrent edit volume overwhelms the 500ms file watcher, resulting in race conditions and dropped vault syncs.
**Blast Radius** -> Severe end-to-end latency degradation, dropped memory clusters, out-of-sync cognitive artifacts, and catastrophic write amplification leading to system instability.
**Recommended Structural Change** ->
1. Migrate from a monolithic SurrealKV node to a distributed, sharded persistence layer.
2. Replace hardcoded token budgets with a hierarchical clustering mechanism with dynamic, recursive summarization.
3. Replace the local 500ms file watcher with a highly available, event-driven Change Data Capture (CDC) pipeline.
