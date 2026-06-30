---
title: "Architecture Review: Persistent Lock Retry Loop Hides Fundamental Contention"
labels: ["architecture-review", "adversarial"]
---

### Finding
Persistent Lock Retry Loop Hides Fundamental Contention

### Current Assumption
Wrapping RocksDB/SurrealKV connection acquisition in a retry loop (up to 9/10 attempts, 500ms sleep) resolves multi-process lock contention gracefully.

### Attack Scenario
Under high concurrency (e.g., a burst of agent spawns or background compaction sweeps overlapping with client queries), the 5-second backoff window is easily exceeded. Adversarial rapid restarting of clients or aggressive parallel test executions will exhaust the retries, causing cascading lock acquisition failures.

### Blast Radius
Complete state paralysis. The daemon or SDK clients fail to initialize, silently dropping memory writes or crashing the application due to failed DB bootstrapping.

### Recommended Structural Change
Abandon file-lock-based multi-writer contention. Implement a single-writer, multi-reader architecture. The daemon must hold the exclusive database lock indefinitely, and all clients must route reads/writes through the daemon via a lightweight IPC mechanism (e.g., gRPC over Unix Domain Sockets) rather than contending for filesystem locks.

> **Note:** Do not close this issue without a documented Architectural Decision Record (ADR) response.