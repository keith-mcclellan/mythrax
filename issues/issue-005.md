---
title: "Architecture Review: Tight Coupling Between Vault Watcher and Database Ingestion"
labels: ["architecture-review", "adversarial"]
---

**Finding:** Tight coupling between the Obsidian Vault Watcher and synchronous database ingestion creates a denial of service vector.

**Current Assumption:**
As documented in `ARCHITECTURE.md`: "500ms File Watcher Coalescing: The Obsidian vault watcher utilizes the `notify` crate to detect file edits. To prevent high-frequency write cascades and ingestion races, events are coalesced over a 500ms sliding window before being committed to the database." This assumes that a simple time-based window is sufficient to absorb heavy filesystem I/O without blocking the main daemon's ingestion pipeline.

**Attack Scenario:**
An attacker (or a runaway script/tool) continuously touches or modifies thousands of files in the Obsidian vault at high frequency. The `notify` crate generates a massive stream of filesystem events. Because the coalescing logic is directly tied to the database commit pipeline, this massive event flood exhausts the 500ms window processing capacity, blocking the database connection and inducing severe lock contention on SurrealKV/RocksDB.

**Blast Radius:**
The database transaction queue stalls. As a result, the daemon cannot ingest new episodic memories or commit agent handoffs, effectively freezing all cognitive functions and state updates.

**Recommended Structural Change:**
Decouple the Vault Watcher from the database transaction pipeline. Introduce an asynchronous, bounded-capacity message queue (e.g., using Rust channels or a lightweight broker) between the filesystem event listener and the database ingestion workers. If the queue fills up, shed load or aggregate events without blocking the core database writer.
