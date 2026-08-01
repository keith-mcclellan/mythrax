---
title: "Bug: Unbounded Memory Exhaustion and DoS via Sliding Window Caps in Graph Traversal"
labels: ["bug", "agent-found"]
severity: "Critical"
---

## Bug Description
In `mythrax-core/src/db/crud_operations.rs` around lines 2680-2750, `query_symbolic_scored_db` performs a BFS graph traversal. It attempts to bound resource usage by placing a `LIMIT 50` constraint per query at each hop. However, the sliding window cap is unbounded across levels. A depth-3 traversal branching 50 edges at each level will yield $50^3 = 125,000$ nodes in memory. This leads to an unbounded recursion/memory combinatorial explosion, which can lock the single-thread async executor and cause OOM crashes (denial-of-service via adversarial memory graphs).

## File & Line Number
`mythrax-core/src/db/crud_operations.rs:2680-2750`

## Minimal Reproducible Scenario
1. Construct an adversarial memory graph where a single starting node connects to 50 distinct nodes.
2. Ensure each of those 50 nodes connects to 50 distinct nodes, recursively up to depth 3 or 4.
3. Trigger a symbolic query traversal (e.g., `query_symbolic_scored_db` with `max_depth` set to 3 or higher).
4. The BFS queue quickly swells to $125,000+$ items, leading to severe CPU locking and potential OOM.

## Suggested Fix
1. Limit the maximum size of the BFS queue explicitly (e.g. `if queue.len() > MAX_QUEUE_SIZE { break; }`).
2. Alternatively, limit the total number of edges processed across all depths, not just 50 per individual node expansion.
