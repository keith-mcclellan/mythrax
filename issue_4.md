---
title: "Fragile Iterator Invalidation and Panic Path in Memory Eviction"
labels: ["bug", "agent-found"]
---

## Description
In the LRU paging manager, an element is removed from a deque while iterating via an index range. An unnecessary `.unwrap()` is invoked on the removal operation which could lead to panics if the queue shifts unpredictably. While the `break` statement currently masks the immediate bounds error, it represents a fragile logic flaw.

## Location
`mythrax-core/src/cognitive/memory_os.rs`, lines 38-48

## Minimal Reproducible Scenario
The `evict_if_needed` function uses:
```rust
for i in 0..self.lru_queue.len() {
    let id = &self.lru_queue[i];
    if let Some(info) = self.active_nodes.get(id) {
        if !info.pinned {
            let id_to_evict = self.lru_queue.remove(i).unwrap();
            // ...
            break;
        }
    }
}
```
If logic is later changed to remove multiple candidates at once (removing the `break`), this will trigger an out-of-bounds removal. `remove(i)` returns an `Option`, and unwrapping it is a hazardous panic path.

## Severity
Medium (Off-by-one / iterator invalidation risk).

## Suggested Fix
Gracefully handle the `Option` returned by `VecDeque::remove`, which also clearly documents that `remove` might fail:

```rust
if let Some(id_to_evict) = self.lru_queue.remove(i) {
    self.active_nodes.remove(&id_to_evict);
    evicted.push(id_to_evict);
    found_candidate = true;
    break;
}
```
Alternatively, decouple finding the candidate index from mutating the underlying structure.
