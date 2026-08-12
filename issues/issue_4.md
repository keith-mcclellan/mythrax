---
title: "Architecture Liability: Agent Auto-Spawn Race Conditions"
labels: ["architecture-review", "adversarial"]
---

**Finding** -> Clients automatically spawn the daemon if not running and write the PID to `~/.mythrax/daemon.pid` after port polling.

**Current Assumption** -> Multiple clients will not simultaneously attempt to spawn the daemon, and port polling is a reliable readiness check.

**Attack Scenario** -> In a highly concurrent multi-agent startup scenario, multiple agents fail the port check simultaneously and all attempt to spawn the daemon. This leads to multiple daemon instances binding to the same port or thrashing the PID file.

**Blast Radius** -> Corrupted PID file, locked database files (SurrealKV/SQLite contention), and failure of the system to initialize.

**Recommended Structural Change** -> Use file-based locking (flock) on a socket or lockfile during daemon startup to ensure single-instance exclusivity. Return a backoff response to concurrent clients.

*Note: Do not close this issue without a documented architectural decision record (ADR) response.*
