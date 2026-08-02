---
title: "Unbounded Recursion Risk in Temporal Graph Traversal"
labels: ["architecture-review", "adversarial"]
---

## Finding
In `mythrax-core` (`db/crud_operations.rs`), temporal expansion graph traversals use Sliding Window Caps (e.g., 1,000-element `VecDeque`) and `LIMIT 50` constraints per hop level. However, a depth-3 traversal can still yield up to 125,000 nodes ($50 \times 50 \times 50$).

## Current Assumption
The assumption is that `LIMIT 50` at each hop is sufficient to bound memory usage and processing time during graph traversal.

## Attack Scenario
An attacker deliberately crafts a dense cluster of interconnected memories (e.g., by repeatedly triggering specific actions or storing recursive relationships). When a retrieval operation triggers a depth-3 temporal expansion, the system attempts to process up to 125,000 nodes. This causes an explosion in memory allocation and CPU cycles, leading to unbounded recursion or severe latency bottlenecks.

## Blast Radius
**High.** Denial of Service (DoS) and potential Out-Of-Memory (OOM) crashes. Processing a 125,000-node graph blocks the active thread and exhausts system memory, taking down the unified API gateway and halting all daemon operations.

## Recommended Structural Change
1. **Global Visited Set & Hard Limit:** Implement a global `visited` set and a strict overall hard limit (e.g., max 500 nodes total) across the *entire* traversal, rather than just per-hop limits.
2. **Traversal Depth Caps:** Dynamically restrict the traversal depth based on current system load or query complexity.
3. **Lazy Graph Loading:** Return cursors for graph exploration rather than attempting to materialize the entire expanded neighborhood in memory at once.