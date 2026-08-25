---
tags: architecture-review, adversarial
---

# Data Corruption Risk due to 500ms Shutdown Sleep

**Finding**: The 5-Second Graceful Shutdown Sequence strictly sleeps for 500ms to wait for active IO and DB writes to complete.

**Current Assumption**: Active disk I/O, SQLite embedding cache flushes, and SurrealKV WAL syncs will reliably complete within exactly 500ms during any shutdown event.

**Attack Scenario**: Under heavy load, an attacker forces a SIGTERM or triggers the cancellation token. Because the system merely sleeps for 500ms instead of awaiting actual completion signals, the sleep expires before slow disk writes finish, and the process aggressively exits.

**Blast Radius**: Unrecoverable state corruption. Corrupted SQLite embedding cache, half-written Obsidian markdown files, and irrecoverable SurrealKV state.

**Recommended Structural Change**: Replace the hardcoded 500ms sleep with a deterministic two-phase shutdown. Await actual `JoinHandle` completion from the DB connection and I/O tasks with a bounded but longer timeout (e.g., 5 seconds max per task) rather than a blind sleep.
