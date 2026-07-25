---
title: "18-Month Scaling Projection: Top 3 Liabilities for 10x Scale"
labels: [architecture-review, adversarial]
---

**Finding:** The current architecture contains decisions that will require major rewrites if the system scales 10x in 18 months.

**Current Assumption:** The system will primarily operate as a localized, single-tenant entity where file DBs, static tokens, and local GPU bounds are acceptable limits.

**Attack Scenario:** Scaling 10x requires handling multi-tenant concurrency, distributed agent orchestration, and throughput that single-node file locking cannot support.

**Blast Radius:** Architectural gridlock. Scaling beyond a single robust node will necessitate complete rewrites of the persistence, orchestration, and inference layers.

**Recommended Structural Change:** Address the top 3 scaling liabilities: 1) Replace RocksDB/SurrealKV file locks with a distributed database system capable of concurrent high-throughput ingestion. 2) Decouple tightly-coupled in-process MLX inference to external microservices to horizontally scale GPU capacity. 3) Redesign the Single-Port Gateway from a static shared token to a robust API gateway with dynamic auth, rate limiting, and multi-tenant isolation.
