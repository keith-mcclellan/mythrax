---
title: "Red Team Architecture Brief: 18-Month Scaling Liabilities and Single Point of Bottleneck"
labels: ["architecture-review", "adversarial"]
---

**Finding:** The current Mythrax architecture exhibits critical 18-month scaling liabilities, specifically its reliance on local file DB locks (RocksDB/SurrealKV), tightly-coupled in-process GPU inference, and a vulnerable single-port gateway design.

**Current Assumption:** The assumption is that the system will primarily run as a localized sidecar daemon with a moderate throughput of sequential or lightly concurrent agent requests, and that local NVMe storage and single-node GPUs provide sufficient bandwidth.

**Attack Scenario:** The system scales 10x to support multiple concurrent agent swarms across distributed workspaces.
1. The reliance on local file DB locks (SurrealKV/SQLite) causes severe contention and transaction timeouts under heavy parallel read/write loads from multiple agents.
2. The tightly-coupled in-process GPU inference (Metal GPU) cannot scale horizontally, leading to massive queuing delays and timeouts for embedding and routing tasks.
3. The single-port API gateway (Port 8090) becomes a catastrophic network bottleneck and a single point of failure; a localized DoS attack or heavy traffic spike takes down the entire orchestration layer.

**Blast Radius:** Complete systemic failure under load. The architecture cannot gracefully degrade; it will instead suffer from cascading lock timeouts, OOM crashes due to queued inference tasks, and gateway unavailability, rendering all connected AI agents non-functional.

**Recommended Structural Change:**
These three decisions will require re-architecture within 18 months:
1. **Database:** Migrate from local file-locked databases (RocksDB/SurrealKV) to a distributed, highly concurrent data store (e.g., PostgreSQL or a dedicated remote vector database) to eliminate lock contention.
2. **Inference:** Decouple the in-process model broker. Transition to a stateless, horizontally scalable RPC microservice for embeddings and small model inferences, allowing separate scaling of compute and memory nodes.
3. **Gateway:** Replace the single-port gateway with a robust, load-balanced API ingress layer that supports rate limiting, circuit breaking, and traffic shaping to prevent catastrophic bottlenecks.
