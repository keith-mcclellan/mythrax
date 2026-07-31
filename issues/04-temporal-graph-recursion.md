---
title: "Unbounded Recursion Risk in Temporal Graph Expansion"
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief

**Finding:**
`query_symbolic_scored_db` in `mythrax-core/src/db/crud_operations.rs` uses a 1,000-element maximum `hits` bound and a depth-3 limit, but queries up to 50 items per hop via `LIMIT 50`.

**Current Assumption:**
The per-hop `LIMIT 50` and depth limit of 3 are sufficient to prevent unbounded memory usage and database utilization during traversal.

**Attack Scenario:**
An attacker crafts a densely interconnected set of memory nodes (a "memory bomb"). A depth-3 traversal with a branching factor of 50 can yield up to $50^3 = 125,000$ potential traversals before the 1,000 hit limit is reached, as the BFS queue can grow massively. This leads to CPU exhaustion, memory bloat, and database lock contention as thousands of queries are fired.

**Blast Radius:**
Denial-of-service, database locks, and daemon freeze, blocking all concurrent agent requests.

**Recommended Structural Change:**
Implement strict total hop bounds and global query quotas rather than just per-hop limits. Refactor the graph traversal to utilize native GraphDB optimizations (like SurrealDB graph queries) instead of application-level BFS looping, or enforce a strict global timeout for graph expansion.
