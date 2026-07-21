---
labels: ["architecture-review", "adversarial"]
---

# Issue: 18-Month Scaling Liability - Local File DB Locks

## Finding
As documented in `ARCHITECTURE.md`, the system relies on local file database engines (SurrealKV and RocksDB) that require exclusive file locks. To mitigate contention errors during rapid daemon restarts or multi-process access, the architecture employs a "Persistent Lock Retry Loop" (up to 10 attempts with 500ms sleep).

## Current Assumption
The architecture assumes that the system will primarily run as a single instance (or a small number of instances) where lock contention is transient and can be resolved by a simple backoff-and-retry strategy. It assumes the volume of concurrent read/write operations will remain low enough that retry loops do not introduce unacceptable latency.

## Attack Scenario
1. **Accidental Denial of Service (Scale):** As the system scales 10x over the next 18 months, handling concurrent requests from dozens of agents, the retry loops will inevitably saturate. Multiple processes attempting to write simultaneously will exhaust their retry limits, leading to transaction failures, dropped memories, and system instability.
2. **Adversarial Denial of Service:** An attacker (or a compromised agent) could intentionally spam the database with rapid, small write requests, keeping the database file locked. Legitimate processes will get caught in the 500ms backoff loop, effectively DoSing the entire unified gateway and paralyzing all agents relying on the persistent cognitive graph.

## Blast Radius
**System Paralysis.** When the retry limits are exceeded, database operations fail. Because all agents rely on the shared SurrealKV/RocksDB file for context, handoffs, and memory, the entire system grinds to a halt. There is no graceful degradation; agents will either panic, fail to retrieve context, or lose episodic memories.

## Recommended Structural Change
1. **Migrate to a Client-Server Database Model:** To support 10x scaling, replace the exclusive local file lock databases (SurrealKV/RocksDB) with a robust, concurrent client-server database architecture (e.g., PostgreSQL, or a dedicated SurrealDB server instance).
2. **Implement Connection Pooling:** Utilize proper connection pooling and transaction management to handle high concurrency without relying on naive file-level locking and arbitrary sleep loops.

*Note: Do not close this issue without a documented Architectural Decision Record (ADR) response.*