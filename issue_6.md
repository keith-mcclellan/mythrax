---
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief: Tight Coupling of Database Lock Retry & Daemon Startup

**Finding:** The Persistent Lock Retry Loop wraps the dual-engine storage (SurrealKV/RocksDB) initialization in a retry loop (up to 9/10 attempts, 500ms sleep) to handle lock contention during rapid daemon restarts.

**Current Assumption:** Transient file locks are the only cause of contention, and a short 5-second backoff loop is sufficient to guarantee exclusive access to the underlying RocksDB/SurrealKV storage.

**Attack Scenario:** In a high-concurrency environment or if a previous process crashes without releasing the lock cleanly (or if an adversarial process holds the file lock intentionally), the 5-second retry loop will exhaust. The daemon will fail to start.

**Blast Radius:** Denial of Service (DoS) for the entire Mythrax ecosystem. If the daemon cannot acquire the database lock, it panics on startup. Clients relying on the auto-spawn sequence will continuously fail to connect, leading to a complete system outage. This coupling prevents independent scaling of the daemon and storage.

**Recommended Structural Change:** Decouple the database from the daemon process by migrating to a standalone database server model (e.g., standard SurrealDB server) instead of embedded file-locked engines, OR implement robust lock-stealing/dead-process-detection logic to forcefully clear stale locks if the holding PID is dead. Require a mandatory ADR response to close this issue.