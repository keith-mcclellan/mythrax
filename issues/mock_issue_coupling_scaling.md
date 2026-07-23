# [HIGH] Tight Coupling & Scaling Liability of Local File Locks

**Labels:** `architecture-review`, `adversarial`, `scalability`

## Finding
The system heavily relies on local embedded file databases (RocksDB/SurrealKV) with exclusive file locks, tightly coupled to the in-process execution.

## Current Assumption
RocksDB and SurrealKV can scale efficiently using exclusive local file locks for persistence, and dynamic fallback/retry loops (up to 10 attempts) are sufficient to handle lock contention.

## Attack Scenario (Load)
As the system scales to 10x concurrent agents, API load, or background compaction sweeps, file lock contention will exponentially increase, causing the retry loop to fail. This will lead to dropped transactions, database corruption, or complete daemon lockups (as currently evidenced by `cargo test` timeouts).

## Blast Radius
Total database unavailability, dropped memories, and system deadlock across all active agents. Failure has no graceful degradation path.

## Recommended Structural Change
Decouple the database from the local file system. Transition to a dedicated remote/distributed database instance (e.g., a clustered SurrealDB instance or Postgres via gRPC) to handle concurrent multi-process access instead of relying on fragile local file locks.

**Note:** Do not close this issue without a documented Architectural Decision Record (ADR) response.
