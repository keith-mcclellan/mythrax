# Coupling: Streaming-to-Disk Pipeline and Obsidian Vault Watcher

**Labels:** architecture-review, adversarial

**Finding:** The cognitive pipeline streams artifacts to the canonical Obsidian Vault markdown files. Concurrently, a file system watcher monitors the vault (500ms coalescing). If the pipeline writes trigger the watcher to re-ingest, it creates a tight coupling.

**Current Assumption:** The cognitive pipeline and the vault watcher can coexist peacefully if the pipeline suppresses raw episode `.md` flushes and the watcher relies on 500ms coalescing.

**Attack Scenario:** A bug in path filtering or a rapidly fluctuating synthesis loop causes the pipeline to rapidly overwrite markdown files. The watcher picks up these changes and re-ingests them into SurrealDB, which triggers another compaction sweep, leading to an infinite write-ingest loop.

**Blast Radius:** Unbounded disk I/O, database bloat, and CPU exhaustion, rendering the daemon unusable.

**Recommended Structural Change:** Decouple the write pipeline from the read watcher via distinct staging directories or cryptographic watermarks (e.g., appending a specific metadata tag that the watcher explicitly ignores) to guarantee unidirectional data flow without relying solely on path exclusions.
