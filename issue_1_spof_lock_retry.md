# 💥 SPOF & Architectural Assumption Challenge: Persistent Lock Retry Loop

**Tags:** `architecture-review`, `adversarial`

**Requires ADR response to close.**

**Finding:**
The persistent lock retry loop and exclusive file locking mechanism for RocksDB/SurrealKV is a critical single point of failure (SPOF) with no graceful degradation path.

**Current Assumption:**
*Architecture.md* assumes that multi-process contention or rapid daemon restarts will be transient, and that wrapping the database connection in a "retry loop with backoff (up to 10 attempts, 500ms sleep)" is sufficient to handle pending lock releases. It also assumes that `replay_wal_if_fresh` and WAL background actors will always have unhindered exclusive access post-boot.

*What assumption does this break if it's wrong?* It assumes locks are *only* held temporarily. If a lock is permanently orphaned due to an abrupt kill signal not caught by the graceful shutdown, or if filesystem latency on a networked drive exceeds the 5-second total retry window, the database will refuse to start.

**Attack Scenario:**
Under heavy load, multiple concurrent agents or a panicked process leaves a dangling `.lock` file. The daemon restarts, retries 10 times over 5 seconds, and panics because the lock isn't released. The entire daemon crashes. Alternatively, an adversarial script rapidly connects/disconnects, intentionally saturating the lock timeout.

**Blast Radius:**
Complete Denial of Service (DoS) for the entire Mythrax daemon. Memory ingestion, retrieval, model inference, and WAL replay all halt. Since this is at the entry point (`SurrealBackend::new`), there is zero graceful degradation—the system cannot serve cached reads or accept queued writes.

**Recommended Structural Change:**
Abandon exclusive file-based locking (RocksDB) in favor of a concurrent-friendly architecture (e.g., embedded SQLite with WAL mode, Postgres, or a lock-leasing mechanism with heartbeat timeouts). If sticking to SurrealKV, implement a highly available, out-of-process persistence tier that handles connection multiplexing rather than giving the daemon exclusive file ownership.
