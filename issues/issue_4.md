---
title: "High Coupling in Graph Iteration"
labels: ["architecture-review", "adversarial"]
---

Finding -> Current Assumption -> Attack Scenario -> Blast Radius -> Recommended Structural Change

**Finding**: Iterative graph operations rely on string cloning and HashMap lookups, tightly coupling algorithmic logic to string representations.
**Current Assumption**: The memory graph will remain small enough that string hashing overhead is negligible.
**Attack Scenario**: The system scales 10x, accumulating hundreds of thousands of nodes. Iterations become CPU-bound and block the async executor.
**Blast Radius**: Daemon latency spikes and API gateway starvation.
**Recommended Structural Change**: Map node identifiers to contiguous integers and use flat vectors (CSR format) for tight inner loops.

**Note:** Never close this issue without a documented architectural decision record (ADR) response.
