---
title: "🛡️ Sentinel: [HIGH] Unbounded Recursion Risk in Temporal Expansion Graph Traversals"
labels: ["architecture-review", "adversarial", "bug", "agent-found"]
---

### Finding:
In `mythrax-core` (specifically `db/crud_operations.rs`), the use of Sliding Window Caps and `LIMIT 50` constraints per hop level during temporal expansion graph traversals creates an unbounded recursion risk. A depth-3 traversal can yield 125,000 nodes.

### Current Assumption:
The assumption is that `LIMIT 50` per hop is small enough to prevent memory and processing explosion.

### Attack Scenario:
An adversary intentionally creates a highly dense graph of memories or triggers operations that generate numerous related nodes. When a temporal expansion is requested (e.g., during memory retrieval or clustering), the depth-3 traversal yields up to 125,000 nodes, exhausting memory and CPU, leading to a Denial-of-Service (DoS) crash of the daemon.

### Blast Radius:
HIGH. Denial of Service. The intelligence daemon becomes unresponsive or crashes due to OOM/CPU exhaustion, halting all agent operations.

### Recommended Structural Change:
- Implement a global hard limit on the total number of nodes retrieved across all hops during graph traversal, regardless of the per-hop limit.
- Introduce cyclic dependency detection during traversal.
