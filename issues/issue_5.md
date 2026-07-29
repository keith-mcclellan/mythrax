---
title: Unbounded recursion / DoS via Sliding Window Caps in temporal expansion
labels: bug, agent-found
severity: Critical
---

**File/Line:** `mythrax-core/src/db/crud_operations.rs` : 2584-2619

**Minimal Reproducible Scenario:**
In the temporal expansion graph traversal (`VecDeque::new()` at line 2584), the code limits per-hop queries to `LIMIT 50` (lines 2607, 2609, 2616, 2618) and uses a `limit_depth = max_depth.unwrap_or(3)` but lacks a global traversal count limit. With a depth of 3 and branching factor of 50, a single traversal could expand to 125,000 nodes in the queue, leading to high latency or memory exhaustion (DoS).

**Suggested Fix:**
Maintain a global counter of visited nodes and terminate the BFS if it exceeds a safe limit (e.g., 1000 nodes).
```rust
let mut queue = VecDeque::new();
let mut visited_count = 0;
while let Some(...) = queue.pop_front() {
    if visited_count > 1000 { break; }
    visited_count += 1;
    // ...
```
