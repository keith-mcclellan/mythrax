---
title: "Persistent Lock Retry Loop enables Denial of Service"
tags: [architecture-review, adversarial]
---

# Finding: Persistent Lock Retry Loop enables Denial of Service

## Current Assumption
The architecture assumes that RocksDB/SurrealKV lock contention is a transient state (lasting <5 seconds) and can be resolved by a 10-attempt retry loop with 500ms backoff.

## Attack Scenario
An adversarial process, an agent bug (unbounded recursion causing rapid spawn cycles), or a daemon panic while holding the lock can cause the lock file to be held indefinitely.

## Blast Radius
Total System Failure. The daemon fails to boot or serve any database-backed requests (memory, configuration, tool calls). There is no graceful degradation path—the retry loop simply exhausts and the system panics or hangs.

## Recommended Structural Change
Replace the embedded, exclusive file-locking database model with a true client-server database architecture (e.g., PostgreSQL/pgvector or a standalone SurrealDB server) for concurrent access, or implement a heartbeat-based lock lease mechanism that automatically evicts stale locks from crashed processes.

*This issue requires an ADR response to close.*
