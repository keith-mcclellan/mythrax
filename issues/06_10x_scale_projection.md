---
title: "🛡️ Sentinel: [MEDIUM] Architecture 18 Months Forward: Top 3 Redesign Projects for 10x Scale"
labels: ["architecture-review", "adversarial"]
---

### Finding:
Projecting the architecture 18 months forward for a 10x scale increase, several current design decisions will become critical bottlenecks requiring significant re-architecture.

### Current Assumption:
Current components are sufficient for the expected load and concurrency levels.

### Attack Scenario (Scale/Load Failure):
Under 10x scale, the system will experience severe contention and bottlenecks:
1. **SQLite Embedding Cache (`embeddings.db`) I/O locks:** Increased embedding generation will cause high lock contention, slowing down or blocking all cognitive pipeline operations.
2. **Single-Port API Gateway shared `reqwest::Client` contention:** All traffic flowing through a single port with a shared client will exhaust file descriptors and socket pools, leading to request drops.
3. **Streaming-to-Disk Cognitive Pipeline writing to Obsidian Vault markdown files:** High-frequency file I/O operations will bottleneck the file system, causing delays in memory persistence and sync issues.

### Blast Radius:
MEDIUM. Performance degradation, timeouts, and potential data loss under high load.

### Recommended Structural Change:
- **Embeddings:** Migrate from SQLite to a dedicated, high-concurrency vector database or an in-memory distributed cache with asynchronous disk flushing.
- **API Gateway:** Implement a robust API gateway with connection pooling, load balancing across multiple worker processes, and dedicated ports for different traffic types (e.g., admin, models).
- **Cognitive Pipeline:** Decouple the file system writes from the critical path using a message queue or event bus, batching disk writes asynchronously.
