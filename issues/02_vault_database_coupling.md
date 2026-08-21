# Red Team Architecture Brief: Vault Watcher and Database Coupling

**Labels:** `architecture-review`, `adversarial`
**Requirement to Close:** A documented architectural decision record (ADR) response must be provided.

**Finding:** There is hard coupling between the Obsidian Vault filesystem watcher (`WatchIgnoreList` and `mpsc::unbounded_channel` in `mythrax-core/src/vault/watcher.rs`) and the persistent database state.
**Current Assumption:** The filesystem watcher will not race with database transactions, and the event queue is sufficient to prevent event dropping under load.
**Attack Scenario:** An adversarial process or misbehaving agent writes thousands of files rapidly. The watcher drops events, or the memory buffer overflows, causing a silent split-brain between the filesystem (ground truth) and the vector DB.
**Blast Radius:** Permanent cognitive corruption and desynchronization. The LLM hallucinates on stale or dropped memory graphs.
**Recommended Structural Change:** Decouple filesystem events from the database via an append-only Write-Ahead Log (WAL) or event queue (e.g., Kafka/Redis streams) to ensure reliable asynchronous ingestion.
