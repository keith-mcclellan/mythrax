---
title: "Bug: Inefficient Array Removal in PagingManager Eviction Loop"
labels: ["bug", "agent-found"]
severity: "Medium"
---

## Bug Description
In `mythrax-core/src/cognitive/memory_os.rs:39-44`, the `evict_if_needed` function iterates over `self.lru_queue` using an indexed `for` loop (`for i in 0..self.lru_queue.len()`). When it finds an unpinned node, it removes it from the queue using `self.lru_queue.remove(i)`. While the loop correctly breaks immediately after removing an item (avoiding an index out-of-bounds panic), the process of removing an element from a `VecDeque` and shifting elements is $O(N)$. Because the outer loop continuously restarts the eviction process to meet the capacity constraint, the algorithm degrades to $O(N^2)$ in worst-case scenarios where many unpinned nodes need eviction.

## File & Line Number
`mythrax-core/src/cognitive/memory_os.rs:39-44`

## Minimal Reproducible Scenario
1. Add `100,000` unpinned active nodes to the `PagingManager`, exceeding capacity significantly.
2. Trigger `evict_if_needed()`.
3. The method must repeatedly iterate the queue and `remove(i)` individually, causing a major performance bottleneck due to $O(N^2)$ complexity.

## Suggested Fix
Refactor the eviction loop to filter out evicted nodes in a single pass $O(N)$. For example, using `.retain()` or building a new `VecDeque` that excludes the evicted items up to the required eviction count.
