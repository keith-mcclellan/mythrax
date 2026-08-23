---
title: "SurrealKV and SQLite Persistence Risk: Concurrent Access and File Locks"
labels: ["architecture-review", "adversarial"]
---

## Finding
Mythrax uses a hybrid storage architecture relying on SurrealKV and SQLite with a persistent lock retry loop for multi-process execution.

## Current Assumption
File locks with exponential backoff (up to 10 attempts, 500ms sleep) will adequately manage concurrent database access from multiple processes or during daemon restarts without data corruption or deadlocks.

## Attack Scenario
Under high load or adversarial inputs (e.g., rapid, simultaneous connection requests from multiple spawned agents), the retry loop can be exhausted, leading to database lockouts, request failures, and potential denial of service. If a process crashes while holding a lock, it may leave the database in a locked state, requiring manual intervention or causing cascading failures.

## Blast Radius
Data layer unavailability, leading to system-wide failure of cognitive and memory functions. Potential database corruption if locks are not handled atomically during abrupt termination.

## Recommended Structural Change
Transition to a robust, connection-pooling database architecture (like PostgreSQL or a managed SurrealDB instance) that natively handles concurrent access, connection management, and transaction isolation without relying on file-based locks.

> Note: This issue requires a documented architectural decision record (ADR) response before it can be closed.
