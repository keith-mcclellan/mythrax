---
title: Unbounded recursion risk in temporal expansion graph traversal
labels: bug, agent-found
---

**File:** `mythrax-core/src/db/crud_operations.rs`
**Line:** 2570

**Description:**
The use of a Sliding Window Cap (e.g., 1000 element `VecDeque` via `hits.len() >= 1000` break condition) combined with `LIMIT 50` constraints per hop level during temporal expansion graph traversals can cause an unbounded recursion/processing risk. A depth-3 traversal can yield 125,000 nodes before hitting the hits limit, leading to potential denial-of-service via adversarial memory graphs.

**Minimal Reproducible Scenario:**
Create a densely connected graph of episodes (e.g., node A connects to 50 nodes, each of which connect to 50 nodes, each of which connect to 50 nodes). Perform a temporal expansion query from node A with max_depth 3. The `LIMIT 50` restricts each hop, but branching causes exponential growth, resulting in `50^3 = 125,000` iterations pushed to the queue before evaluating the limit, causing CPU/Memory exhaustion before the loop breaks at `hits.len() >= 1000`.

**Severity:** High (Denial of Service)

**Suggested Fix:**
Limit the size of the queue itself or implement an overall visited node cap to prevent evaluating massive branch factors before breaking.

```rust
        while let Some((current, depth, current_conf)) = queue.pop_front() {
            if hits.len() >= 1000 || path_conf.len() >= 1500 { // Implement explicit global state limits
                break;
            }
```
