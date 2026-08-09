# [architecture-review] 18-Month Projection: Top 3 Scaling Bottlenecks (10x Scale)

**Finding:**
Projecting the current architecture 18 months forward at a 10x scale reveals three critical decisions that will necessitate complete re-architecture.

**Current Assumption:**
The current monolithic daemon, local MLX evaluation, and file-system-based markdown storage are sufficient for the expected load.

**Attack Scenario / Failure Mode (At 10x Scale):**

1.  **Monolithic Daemon & Single DB Lock:**
    *   *Decision:* Routing all read/write/manage operations through a single core daemon (`127.0.0.1:8090`) holding exclusive locks to a single SurrealKV/RocksDB instance.
    *   *Failure Mode:* At 10x concurrent agent sessions, the single SQLite/SurrealDB write lock will become a massive bottleneck, causing severe latency spikes and timeouts across all agents.
2.  **Streaming-to-Disk Obsidian Vault I/O:**
    *   *Decision:* Incrementally flushing human-readable cognitive artifacts (WikiNodes, wisdom rules) to canonical Obsidian Vault markdown and JSON files (`wiki/<scope>/*.md`).
    *   *Failure Mode:* File system I/O latency and indexing overhead for thousands of small markdown files will crush the 500ms file watcher coalescing loop. The disk I/O will become a severe bottleneck for the cognitive pipeline.
3.  **In-Process MLX GPU Bindings:**
    *   *Decision:* Loading models natively into the daemon's process memory using the MLX Metal GPU backend.
    *   *Failure Mode:* As noted in the memory rules, if the MLX C++ bindings panic or OOM (which is increasingly likely with larger models or 10x concurrent requests), it abruptly crashes the *entire daemon*, taking down all connected agent sessions.

**Blast Radius:**
System-wide gridlock, high latency, and frequent daemon crashes under heavy load.

**Recommended Structural Change:**
1.  **Distributed Storage:** Migrate from a single monolithic SurrealKV to a horizontally scalable, distributed database (e.g., FoundationDB or a sharded Postgres cluster).
2.  **Virtual File System / Object Store:** Replace raw filesystem markdown writes with an object store (S3-compatible) or a virtual filesystem layer to handle millions of small artifacts without OS-level file watcher bottlenecks.
3.  **Out-of-Process Inference:** Decouple ML inference entirely. Move the Model Broker out of the core daemon process into a dedicated, isolated microservice container that communicates via gRPC. If the inference service OOMs, the core daemon remains stable.
