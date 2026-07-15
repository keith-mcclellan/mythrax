# Finding: Core Architecture Will Fail Under 10x Scale

**Current Assumption:**
The system relies on single-node storage ("SurrealKV & RocksDB Engines", ARCHITECTURE.md), sequential hardware orchestration ("sequential eviction loop", ARCHITECTURE.md), and localized debouncing ("500ms sliding window", ARCHITECTURE.md) to manage load.

**Attack Scenario:**
At 10x scale (e.g., 100 concurrent agents, massive vault repositories):
1. The 500ms sliding window will thrash the database as hundreds of files change per second, resulting in write amplification and transaction starvation.
2. Sequential VRAM eviction will cause massive queue delays for concurrent inference requests, eventually leading to timeouts.
3. Single-node exclusive file locking prevents horizontal scaling of the daemon across a cluster, creating a hard ceiling on throughput.

**Blast Radius:**
Unusable latency, OOM crashes under parallel load, and inability to support enterprise team deployments or multi-agent swarms.

**Recommended Structural Change:**
1. Replace the 500ms coalesce with a distributed event queue (e.g., Redis/Kafka) for ingestion.
2. Implement tensor parallelism or external dedicated inference endpoints instead of a sequential VRAM eviction queue.
3. Migrate to a distributed SurrealDB cluster (TiKV backend) instead of local RocksDB file locks to allow horizontal Daemon scaling.