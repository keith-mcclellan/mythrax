---
title: "Red Team Architecture Brief: Unbounded Recursion Risk in Cognitive Compaction"
labels: ["architecture-review", "adversarial"]
---

### Finding
The system uses "Sliding Window Caps" (e.g., 1,000-element `VecDeque` for Transcript Tool Sequence Cap) to bound memory. However, the temporal expansion graph traversals apply `LIMIT 50` constraints *per hop level* (depth 1/2/3).

### Current Assumption
The assumption is that capping limits per hop prevents graph explosion and bounds memory usage, while the sliding window ensures long sessions do not cause OOM.

### Attack Scenario
An adversary creates an adversarial graph of memory nodes (e.g., highly connected, recursive tool interactions or dense memory clusters). Even with `LIMIT 50` per hop, a depth-3 traversal yields $50^3 = 125,000$ nodes. If the agent orchestration fails to enforce strict scope boundaries during traversal, or if a prompt generation loop iterates over this artificially dense cluster without a global time/token budget, the system enters unbounded recursion or catastrophic latency bottlenecks (O(N^2) scaling for distance calculations).

### Blast Radius
Denial of Service (DoS) via algorithmic complexity. The compactor sweep service hangs indefinitely, starving the tokio runtime and blocking all other background tasks (like file watchers and model evictions).

### Recommended Structural Change
1. **Global Traversal Limits**: Implement absolute, global limits on the total number of nodes retrieved across all hops, rather than just per-hop limits.
2. **Circuit Breakers**: Implement strict timeout circuit breakers (e.g., `tokio::time::timeout`) on all graph traversal and cognitive synthesis pipelines.