---
title: "Red Team Architecture Brief: Unbounded Recursion in Temporal Expansion Graph Traversals"
labels: ["architecture-review", "adversarial"]
---

**Finding:** The temporal expansion graph traversals apply `LIMIT 50` constraints per hop level (depth 1/2/3) but fail to bound the total number of nodes visited.

**Current Assumption:** The assumption is that a `LIMIT 50` constraint per hop is sufficient to prevent graph explosion and unbounded memory usage during temporal neighbor expansion.

**Attack Scenario:** An attacker intentionally creates a highly dense memory cluster by repeatedly performing related actions or injecting linked concepts. During a depth-3 traversal, the expansion explores 50 nodes at depth 1, which each expand to 50 nodes at depth 2 (2,500 nodes), which each expand to 50 nodes at depth 3 (125,000 nodes). This geometric progression causes an unbounded recursion risk, leading to massive CPU and memory consumption.

**Blast Radius:** Denial-of-Service (DoS) via resource exhaustion. The system will hang or crash due to Out-Of-Memory (OOM) errors, taking down the Mythrax Core Daemon and disrupting all connected agents and API services.

**Recommended Structural Change:**
1. Implement a global node visitation cap (e.g., maximum 500 nodes total) for the entire graph traversal, rather than just a per-hop limit.
2. Introduce a cycle-detection and visited-node set to prevent redundant expansions and infinite loops in cyclic memory graphs.
3. Transition from depth-first or unconstrained breadth-first traversal to a bounded priority queue (e.g., Dijkstra's or A* based on temporal decay scoring) that guarantees termination within a fixed resource budget.
