---
title: "Bug: Off-by-one / Out-of-bounds access during eviction in PagingManager"
labels: ["bug", "agent-found"]
---

### Description
In `mythrax-core/src/cognitive/memory_os.rs` around line 39, `evict_if_needed` iterates over `self.lru_queue.len()` using an index `i`.
Inside the loop, if it finds an unpinned item, it removes it from `self.lru_queue` using `self.lru_queue.remove(i).unwrap()`.
Modifying a collection while iterating over it by index shifts the elements, causing subsequent indices to point to the wrong elements (skipping items) or resulting in an out-of-bounds panic when `i` reaches the original length.

### Minimal Reproducible Scenario
1. Create a `PagingManager` with capacity 1.
2. Add Node A (unpinned).
3. Add Node B (unpinned). The length of `lru_queue` is 2.
4. `evict_if_needed()` starts a loop `for i in 0..2`.
5. `i = 0`: Node A is found and removed. `lru_queue` now has length 1.
6. `i = 1`: The loop tries to access `self.lru_queue[1]`, but the queue only has length 1. Panic occurs.

### Severity
High. This is a logic bug that directly causes a panic (crash) during standard context window management.

### Suggested Fix
Use a `while` loop or iterator methods to find the index to remove without invalidating loop indices. For example:
```rust
let mut found_candidate = false;
let mut index_to_remove = None;
for (i, id) in self.lru_queue.iter().enumerate() {
    if let Some(info) = self.active_nodes.get(id) {
        if !info.pinned {
            index_to_remove = Some(i);
            break;
        }
    }
}
if let Some(idx) = index_to_remove {
    let id_to_evict = self.lru_queue.remove(idx).unwrap();
    self.active_nodes.remove(&id_to_evict);
    evicted.push(id_to_evict);
    found_candidate = true;
}
if !found_candidate {
    break;
}
```
