---
tags: [architecture-review, adversarial]
status: open
---
# Red Team Architecture Brief: 18-Month 10x Scaling Liabilities

**Finding**: The current architecture has three critical decisions that will mandate complete re-architecture if the system scales 10x in concurrency, memory volume, or model size.

**Current Assumption**: The local-first, synchronous, and tightly coupled design choices will scale linearly without fundamental changes.

**Attack Scenario (Scaling Failure)**: Under a 10x load, the system experiences cascading failures due to inherent design bottlenecks rather than external attacks.

**Blast Radius**: Complete system halt, unbounded latency, and out-of-memory crashes.

**Recommended Structural Change**:
These top 3 decisions must be re-architected:
1. **Local File DB Locks (RocksDB/SurrealKV)**: The 500ms file watcher coalescing and up to 10-retry lock loop will fail catastrophically under heavy multi-agent concurrency. Requires migration to a true distributed client-server database architecture.
2. **In-Process GPU Inference (MLX Coupling)**: Tightly coupling the Rust daemon with local MLX models in the same process leads to fatal VRAM OOMs and prevents horizontal scaling. Requires decoupling inference into an independent service cluster (e.g., vLLM or discrete worker nodes).
3. **Single-Port, Single-Token Gateway**: Port 8090 cannot support multi-tenant security, rate-limiting, or granular routing. Requires an enterprise API gateway with OIDC/OAuth2.
Do not close this issue without a documented ADR response.
