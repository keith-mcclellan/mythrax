---
title: Panic vulnerability in memory eviction via unwrap() on Vec::remove
labels: bug, agent-found
---

**File:** `mythrax-core/src/cognitive/memory_os.rs`
**Line:** 43

**Severity:** High (Crash in eviction logic)

**Issue:**
The code attempts to remove an item from `lru_queue` using an index:
`let id_to_evict = self.lru_queue.remove(i).unwrap();`
If `lru_queue` is a standard collection like `VecDeque`, `remove(i)` returns an `Option<T>`. If the index is out of bounds or the collection length changes concurrently, `remove` will return `None`, and the `.unwrap()` will trigger a panic, crashing the memory OS actor.

**Minimal Reproducible Scenario:**
If a concurrent task or race condition alters the `lru_queue` size between the bounds check and the `.remove(i)` call, it will panic. Even without concurrency, the presence of `.unwrap()` on a state-mutation operation violates the strict "Panic & Stub Elimination" production rule.

**Suggested Fix:**
Use safe option matching (e.g. `if let Some(id_to_evict) = self.lru_queue.remove(i) { ... }`) to handle the removal.
