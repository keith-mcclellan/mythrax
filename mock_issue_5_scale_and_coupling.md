---
labels: [architecture-review, adversarial]
---
# Architectural Coupling & 18-Month 10x Scale Projections

**Finding:**
There is a tight architectural coupling across the persistence and ingestion layers that will not survive a 10x increase in agent concurrency over the next 18 months. Specifically, three core architectural decisions made today will become major re-architecture projects:
1. **Persistent Lock Retry Loop:** `SurrealBackend::new` relies on exclusive local file locks (RocksDB) with a synchronous up-to-10 attempt (500ms sleep) retry loop to resolve contention.
2. **500ms File Watcher Coalescing:** The Obsidian vault watcher uses a sliding 500ms window to coalesce file edits before writing to the database.
3. **In-Process vs External Model Routing:** The architecture relies on brittle port-based delegation (port 8080) for large models and metal GPU semaphores for in-process smaller models.

**Current Assumption:**
The architecture assumes that concurrent database accesses, file modifications, and local GPU VRAM allocations will be sparse or transient enough that simple backoffs (retry loops and sliding windows) will gracefully handle the load without stalling the system.

**Attack Scenario (18-Month 10x Scale):**
As the system scales to handle 10x concurrent agent workloads:
- High-frequency memory writes exhaust the 10-attempt DB lock retry loop across multiple threads, completely blocking the database.
- The 500ms coalescing window is overwhelmed by rapid, interleaved agent edits across the vault, causing race conditions and dropped telemetry.
- GPU semaphores deadlock as in-process models compete with external routing for resources.

**Blast Radius:**
Complete system saturation, deadlock, and data loss. The cognitive pipeline stalls, causing agents to time out waiting for database or model responses.

**Recommended Structural Change:**
1. Decouple storage from synchronous local locking by migrating to a concurrency-friendly distributed database or inserting a high-throughput message queue (e.g., Redis/Kafka) in front of the DB.
2. Replace file-watcher coalescing with an event-driven pub/sub architecture for state changes.
3. Decouple inference entirely from the core daemon by shifting to a scalable, stateless microservice model broker.
