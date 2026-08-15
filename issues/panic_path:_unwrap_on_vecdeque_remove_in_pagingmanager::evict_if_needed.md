---
title: Panic Path: Unwrap on VecDeque remove in PagingManager::evict_if_needed
labels: bug,agent-found
---

**File:** `mythrax-core/src/cognitive/memory_os.rs`
**Line:** 43

**Description:**
The code uses `.unwrap()` when removing from a `VecDeque` during eviction:
```rust
let id_to_evict = self.lru_queue.remove(i).unwrap();
```
While the index `i` is obtained from iterating `0..self.lru_queue.len()`, if another thread modifies `lru_queue` or if `remove` behaves unexpectedly with concurrent changes, this will panic. Even if currently single-threaded, it violates the zero-unwrap mandate for production paths.

**Severity:** High

**Minimal Reproducible Scenario:**
Create a `PagingManager` and trigger `evict_if_needed` while simulating concurrent access or modifying the queue during iteration.

**Suggested Fix:**
Use defensive programming to handle the `Option`:
```rust
if let Some(id_to_evict) = self.lru_queue.remove(i) {
    self.active_nodes.remove(&id_to_evict);
    evicted.push(id_to_evict);
    found_candidate = true;
    break;
}
```