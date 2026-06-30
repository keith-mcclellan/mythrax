---
title: "Database Lock Contention via Retry Loops"
labels: ["architecture-review", "adversarial"]
---

**This issue requires a documented Architectural Decision Record (ADR) response to close.**

### Finding
Database Lock Contention via Retry Loops

### Current Assumption
Wrapping SurrealKV/RocksDB connections in a retry loop with exponential backoff (up to 9/10 attempts, 500ms sleep) is sufficient to handle transient lock contention across rapid restarts or concurrent multi-process tests.

### Attack Scenario
An adversarial payload triggers continuous crashing and restarting of the daemon, or multiple misconfigured agents aggressively attempt to spawn the daemon concurrently.

### Blast Radius
Deadlocks and startup failures. The 500ms sleep and retry loop will fail if the system is under sustained contention, leading to database corruption risks or the inability to start the background service at all. The failure has no graceful degradation path.

### Recommended Structural Change
Implement a centralized daemon manager (e.g., a PID file with a socket-based readiness probe) that strictly enforces a single-writer pattern without relying on optimistic file-lock retry loops. Switch to a client-server database architecture instead of embedded RocksDB if multi-process access is required.