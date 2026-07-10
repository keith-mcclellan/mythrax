# 18-Month Scaling Projection: Top 3 Impending Re-Architecture Failures (10x Scale)

**Finding**: Reviewing `ARCHITECTURE.md`, the current Mythrax 2.0 sidecar daemon is heavily optimized for a single-user, local-first environment. If agent concurrency or memory corpus scales by 10x over the next 18 months, three foundational decisions will structurally fail.

### 1. The Single-Port Gateway Monolith (Section 1)
**Current Assumption**: A single REST/MCP Axum router on Port 8090 can handle all throughput.
**Failure at 10x**: Under 10x agent load, heavy MCP tool calls (e.g., massive AST extractions or multi-file diffing) will exhaust the shared thread pool, starving administrative endpoints and causing the daemon to appear dead.
**Attack/Blast**: A DoS attack on the MCP port takes down the entire command and control interface.
**Recommended Change**: Split the Gateway into a high-throughput gRPC/WebSocket data plane and a separate administrative HTTP control plane.

### 2. 500ms File Watcher Coalescing (Section 4)
**Current Assumption**: Sliding window coalescing of 500ms prevents write cascades from Obsidian vault mutations.
**Failure at 10x**: At 10x project complexity, aggressive parallel build systems (e.g., rust-analyzer, tsc) or multi-agent swarms will generate thousands of file events per second. The single coalescing thread will back up, delaying ingestion indefinitely, and potentially overflowing the `notify` channel buffer, causing dropped events and a silent desync between the filesystem and the DB.
**Attack/Blast**: An adversary triggers a massive directory tree modification, causing the watcher to silently drop subsequent critical context updates.
**Recommended Change**: Replace the sliding window with a Kafka-style distributed event log (e.g., Redpanda or NATS JetStream) that can durable queue high-frequency filesystem events for parallel ingestion workers.

### 3. In-Process DBSCAN Daily Dreaming (Section 4)
**Current Assumption**: Running DBSCAN clustering locally during a daily "dreaming" cycle is sufficient to group related episodic memories.
**Failure at 10x**: DBSCAN has a time complexity of O(N^2) without a spatial index. At 10x memory corpus size, the clustering algorithm running inside the main Rust process will block the async executor or cause OOM panics during the dreaming phase, crashing the active daemon.
**Attack/Blast**: An adversary floods the system with low-value episodic memories. The subsequent daily dreaming cycle attempts to cluster millions of nodes, consuming all system RAM and taking down the node.
**Recommended Change**: Offload the DBSCAN/RAPTOR clustering pipeline to an asynchronous, out-of-process batch job framework (e.g., Apache Spark or Ray), reading from a data lake export rather than the live transactional database.

Tags: `architecture-review`, `adversarial`
