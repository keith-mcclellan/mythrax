---
title: "🛡️ Red Team Architecture Brief: Architectural Coupling of Daemon and Local Database (RocksDB/SurrealKV)"
labels: ["architecture-review", "adversarial"]
---

## Red Team Architecture Brief

**Finding:**
The Mythrax 2.0 architecture exhibits tight operational coupling between the core daemon process and the local database storage engines (RocksDB / SurrealKV). The implementation forces exclusive file locks on the database and mandates a retry loop with backoff (up to 10 attempts, 500ms sleep) to manage contention during multi-process execution or daemon restarts.

**Current Assumption:**
The assumption is that the database will always reside strictly on the local filesystem and that simple backoff retries are sufficient to resolve exclusive lock contention. It assumes that the daemon and the storage layer will never need to be independently deployed, scaled across multiple physical nodes, or decoupled for testing purposes.

**Attack Scenario:**
Under heavy parallel load or rapid client spawn sequences (e.g., in a CI/CD pipeline or a multi-agent orchestration framework), multiple processes attempt to acquire the database lock simultaneously. An attacker or a malfunctioning agent rapidly cycles connection attempts, intentionally sustaining the exclusive lock. The 10-attempt retry loop is exhausted across all legitimate client daemon instances.

**Blast Radius:**
Total deadlock and deployment failure. Because the daemon and storage are tightly coupled via exclusive local file locks, failure to acquire the lock prevents the daemon from starting. No agents can access memory, and the system fails to initialize. This tight coupling means the memory tier cannot be independently scaled or tested without spinning up the entire storage layer, creating a major architectural liability as usage grows.

**Recommended Structural Change:**
1. **Abstract the Storage Interface:** Decouple the daemon from the physical file locks by introducing a network-capable storage interface abstraction (e.g., allowing SurrealDB to connect via WebSockets or HTTP for distributed setups, instead of exclusively relying on embedded `surrealkv://` or `rocksdb://`).
2. **Dedicated Connection Manager:** Implement a dedicated, lightweight connection broker or use a SQLite/WAL-style multi-reader approach to eliminate the need for exclusive entire-database file locks.
3. **Graceful Startup Mode:** Allow the daemon to start in an "in-memory only" or "degraded" state if the persistent lock cannot be acquired within the timeout window, queuing writes to the WAL until the lock is available rather than failing completely.

*Note: Do not close this issue without a documented Architectural Decision Record (ADR) response.*