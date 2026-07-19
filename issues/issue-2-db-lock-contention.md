---
tags: [architecture-review, adversarial]
---
# Single Point of Failure: DB Lock Contention

## Finding
The database initializes via a persistent lock retry loop (up to 10 attempts, 500ms sleep) for RocksDB/SurrealKV.

## Code Reference
`mythrax-core/src/db/backend.rs`, within `SurrealBackend::new()`.

## Current Assumption
Transient locks clear within 5 seconds during multi-process operations or rapid restarts.

## Attack Scenario
Under high load or adversarial API spam, frequent requests cause persistent lock contention that outlasts the 5-second window, or a crashed process leaves a stale lock.

## Blast Radius
Total denial of service (DoS) for the daemon. No memory can be ingested or retrieved, and all dependent agents fail to operate or lose state.

## Recommended Structural Change
Transition from exclusive file locking to a robust client-server DB architecture or use an embedded database with better concurrent multi-reader/writer support.

**Note: Do not close this issue without a documented architectural decision record (ADR) response.**
