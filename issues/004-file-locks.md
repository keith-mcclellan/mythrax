---
labels: ["architecture-review", "adversarial"]
---

# Adversarial Architecture Review: 18-Month Scaling Liability of Local File DB Locks

## Finding
Mythrax 3.0 relies entirely on local file database locks via `surrealkv://` and `sqlite` engines. Concurrency is managed via a "Persistent Lock Retry Loop" (exponential backoff up to 10 attempts) to handle multi-process execution or daemon restarts.

## Current Assumption
The architecture assumes that Mythrax will always remain a single-node, single-user "local sidecar daemon," and that 10 retry attempts with a 500ms sleep are sufficient to resolve transient file lock contention.

## Attack Scenario
As the system scales 10x over the next 18 months, the volume of agent memories, streaming cognitive artifacts, and concurrent tool executions will exponentially increase. If multiple agents or parallel Arbor HTR loops attempt concurrent, heavy write operations, the file locks will experience severe contention. Once the 10 retry attempts are exhausted, writes will fail, leading to dropped memories and corrupted cognitive state. An attacker could intentionally trigger a denial-of-service by flooding the MCP endpoints with concurrent memory write requests, intentionally holding the SQLite/SurrealKV locks and blocking all other operations.

## Blast Radius
**Medium to High.** The system fails to scale horizontally. Under heavy load, the database locks become a severe bottleneck, leading to timeouts, dropped data, and system unresponsiveness. The single-node design prevents distributing the load across multiple instances.

## Recommended Structural Change
1. **Client/Server Database Architecture:** Migrate away from exclusive local file locks (`surrealkv://`) toward a true client/server database model (e.g., SurrealDB running as a standalone server, or PostgreSQL) that handles concurrent connections and transaction isolation natively without relying on OS-level file locking.
2. **Asynchronous Message Queueing:** Implement an asynchronous write-ahead queue for memory ingestion. If the database is busy, the daemon queues the write and acknowledges the request, ensuring the API is never blocked by database contention.
