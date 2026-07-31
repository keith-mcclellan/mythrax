---
title: "18-Month Re-architecture Projections at 10x Scale"
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief

**Finding:**
Three major architectural decisions made today will become critical bottlenecks if the system scales 10x in the next 18 months:
1.  **SQLite Embedding Cache (`embeddings.db`)**: Used for high-volume caching.
2.  **Single-Port API Gateway with shared `reqwest::Client`**: Centralized routing and external connection management.
3.  **Streaming-to-Disk Cognitive Pipeline**: Writing directly to Obsidian Vault markdown files (`vault/episodes/*.md`).

**Current Assumption:**
Current load and agent concurrency are low enough to tolerate single-node SQLite I/O, shared HTTP client contention, and direct filesystem writes without hitting OS limits.

**Attack Scenario / Failure Mode:**
*   **Scale 10x**: At 10x concurrency, the SQLite embedding cache will experience severe write-contention and lock I/O bottlenecks.
*   **Scale 10x**: The Single-Port API Gateway will hit connection saturation, leading to TCP socket exhaustion or thread-pool starvation, despite the shared client.
*   **Scale 10x**: Direct, frequent writes to thousands of markdown files will lead to filesystem locks, inode exhaustion, and severe latency in cognitive synthesis pipelines.

**Blast Radius:**
System degrades gracefully until critical OS or I/O thresholds are met, at which point it leads to complete daemon failure, data loss, or unacceptable latency across all agent operations.

**Recommended Structural Change:**
1.  Migrate the embedding cache to a dedicated vector database (e.g., Qdrant, Milvus) optimized for high-throughput concurrent I/O.
2.  Implement an API Gateway with proper load balancing, dynamic scaling, and connection pooling (e.g., Envoy).
3.  Introduce an intermediate message queue (e.g., Kafka, Redis Streams) to buffer cognitive artifacts before asynchronously persisting them to disk.
