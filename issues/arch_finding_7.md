# 7. 18-Month Scaling Projections and Key Vulnerabilities

**Tags**: `architecture-review`, `adversarial`

**Finding**: 7. 18-Month Scaling Projections and Key Vulnerabilities

**Current Assumption**: The current unified monolithic daemon will seamlessly scale for the next 18 months without hitting insurmountable bottlenecks.

**Attack Scenario**: As the agent workload increases 10x, the current architecture will face three critical re-architecture bottlenecks that cannot be patched incrementally: 1. **Persistent Lock Starvation**: The file-based database locks (RocksDB/SurrealKV) will fail under high concurrency. 2. **Monolithic API Gateway Bottleneck**: The single-port router will buckle under the combined load of heavy internal MCP calls and external model routing, leading to connection drops. 3. **Coupled GPU Inference Crashes**: In-process model loading will inevitably lead to an OOM panic that brings down the entire daemon.

**Blast Radius**: Total architectural deadlock within 18 months, requiring a ground-up rewrite of the persistence layer, the network routing layer, and the inference execution model.

**Recommended Structural Change**: Begin planning the decomposition of the Mythrax daemon into a microservices-based control plane: a decoupled PostgreSQL/SurrealDB persistent layer, a distributed API gateway (e.g., Envoy or dedicated edge proxy), and isolated remote inference workers.
