---
title: "Architecture Review: Scaling Projections and Technical Debt Liability (18 Months)"
labels: ["architecture-review", "adversarial"]
---

**Finding:** Current architectural constraints will become critical bottlenecks if system scales 10x over the next 18 months.

**18-Month Scaling Projection & Top 3 Re-Architecture Projects:**

1. **Unified Single-Port Gateway:**
   Currently, port 8090 handles all traffic (REST, MCP, and high-bandwidth completion proxies). Under 10x load, connection pool exhaustion and thread contention will render a single port insufficient. This will require a mandatory re-architecture into a multi-port load-balanced system or placing a reverse proxy (e.g., NGINX/Envoy) in front of dedicated micro-services.

2. **SurrealKV / RocksDB Dual Engine Lock Contention:**
   The reliance on exclusive file locks and a `500ms` backoff loop is already causing contention in multi-process environments. At 10x scale, local file-based exclusive locking will fail under the concurrent read/write pressure of dozens of agents. This will necessitate migrating away from embedded databases toward a distributed database architecture (e.g., dedicated SurrealDB server or PostgreSQL) to manage concurrent transaction isolation efficiently.

3. **500ms Filesystem Coalescing Window:**
   The Obsidian vault watcher relies on a simple 500ms sliding window to prevent ingestion cascades. As vault sizes and concurrent distributed edits grow 10x, a single node trying to coalesce and synchronously commit these events will fall behind. This will demand replacing the simple file watcher with an event-streaming architecture (e.g., Kafka, Redis Streams) to buffer, deduplicate, and asynchronously ingest knowledge updates across multiple nodes.

**Requirement:** An Architectural Decision Record (ADR) response must be documented before this issue can be closed.
