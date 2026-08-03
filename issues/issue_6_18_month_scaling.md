---
title: "Adversarial CTO: 18-Month Scaling Bottlenecks (10x Scale Projection)"
labels: ['architecture-review', 'adversarial']
---

## Finding
18-Month 10x Scale Re-architecture Projects

## Current Assumption
The current hybrid SurrealKV/SQLite architecture and streaming-to-disk Markdown pipeline will smoothly scale 10x over the next 18 months without fundamental redesign.

## Attack Scenario
Under a 10x load of concurrent agent operations and cognitive tasks, three critical architectural decisions will catastrophically fail:
1. **SQLite Embedding Cache (`embeddings.db`) I/O Locks:** Concurrent reads/writes during massive vector operations will cause severe database lock contention.
2. **Single-Port API Gateway `reqwest::Client` Contention:** A shared static client will exhaust file descriptors and socket pools under sustained concurrent proxying.
3. **Streaming-to-Disk Markdown File Pipeline:** Writing heavy Obsidian Vault markdown files directly to disk for every cognitive sync will choke on filesystem I/O, leading to massive write latency.

## Blast Radius
Severe latency degradation, constant I/O bottlenecks, dropped API requests, and failing cognitive syncs across the entire system.

## Recommended Structural Change
1. Replace the SQLite embedding cache with a dedicated distributed vector database or a shared-memory cache designed for concurrent vector I/O.
2. Implement connection pooling, backpressure, and load balancing for API requests.
3. Decouple cognitive syncs from raw filesystem writes; batch markdown disk writes through a dedicated async message queue or use a robust document-oriented database for interim storage. Never close this issue without a documented ADR response.
