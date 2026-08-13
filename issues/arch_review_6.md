---
title: "Finding: Tight Coupling of Gateway and Background Daemon"
labels: ["architecture-review", "adversarial"]
---

**Finding** -> Tight Coupling of Gateway and Background Daemon

**Current Assumption** -> The `mythrax mcp` and CLI thin clients automatically spawning the background daemon if not running is a seamless UX.

**Attack Scenario** -> The auto-spawn logic relies on polling `127.0.0.1:8090` and writing to `~/.mythrax/daemon.pid`. In a multi-tenant or containerized environment, this causes race conditions where multiple thin clients try to spawn the daemon simultaneously, leading to database lock corruption (SurrealKV/RocksDB).

**Blast Radius** -> Database corruption and startup deadlocks. Neither module can be deployed independently without the other's auto-spawn logic interfering.

**Recommended Structural Change** -> Decouple daemon lifecycle management from thin clients. Require explicit daemon startup via systemd/supervisor, or use proper file-locking mechanisms (e.g., `flock`) for the auto-spawn sequence.
