---
title: "Architecture Review: 18-Month Scaling Projections and Imminent Re-Architecture"
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief

**Finding:** Based on the current trajectory, the architecture contains fundamental design flaws that will guarantee system collapse if subjected to a 10x scaling in data volume or concurrent agent orchestration.

**Current Assumption:** The architecture assumes that a single monolithic process running on local hardware (relying on single-port blocking APIs, local embedded RocksDB/SurrealKV, and tightly coupled in-process Metal inference) is sufficient for all future workloads.

**Attack Scenario:** As the system scales 10x over the next 18 months, three decisions made today will become fatal re-architecture projects:
1. **Local Persistent Storage Monolith:** Relying on `SurrealKV/RocksDB` with exclusive file locks (`db_path`) precludes multi-node clustering or distributed agents sharing the same cognitive graph. A single node failure destroys the memory store, and concurrent processes will suffer from massive lock contention (already patched with fragile retry loops).
2. **Synchronous Single-Port Gateway:** Consolidating all API routing, Model Context Protocol (MCP), and completions on a single port (8090) with blocking ML inference calls will bottleneck under heavy load. A simple burst of requests will overwhelm the unified router, causing connection timeouts and cascading failures across agents.
3. **In-Process Memory Embedding Coupling:** Loading `nomic-embed` or small dense models into the same process space as the database and API gateway ensures that a panic or OOM in the ML framework crashes the entire daemon, destroying data in flight.

**Blast Radius:** Inability to scale beyond a single-user, single-machine hobbyist tool. The system will suffer from chronic OOM crashes, database corruption due to lock contention, and unacceptable latency.

**Recommended Structural Change:**
- Shift from exclusive local DB engines to a distributed, network-capable database architecture.
- Break the Single-Port API Gateway into independent microservices (e.g., separating MCP from the LLM proxy).
- Extract all ML/inference capabilities out of the core daemon process into standalone worker nodes communicating via gRPC/IPC.

*ADR required to close this issue.*