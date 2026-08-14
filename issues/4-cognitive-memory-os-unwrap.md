---
title: Panic risk in memory_os LRU queue eviction
labels: ["bug", "agent-found"]
---
## Bug Description
**File and Line Number:** `mythrax-core/src/cognitive/memory_os.rs:43`

**Minimal Reproducible Scenario:** In `evict_if_needed`, the code iterates over `lru_queue.len()` and calls `let id_to_evict = self.lru_queue.remove(i).unwrap();` if the node is not pinned. `VecDeque::remove` shifts elements when an item is removed. While the code immediately breaks out of the loop (`break;`) preventing an immediate out-of-bounds iteration on the *next* step of the current loop, this pattern of removing by index from a `VecDeque` while iterating over its length is extremely brittle. If the `break` is ever moved, removed, or if the logic is refactored to remove multiple items per pass, it will panic due to index out-of-bounds on subsequent iterations. Furthermore, if `i` is somehow out of bounds (due to concurrent mutation, though this is a `&mut self` method so it's synchronously safe), `remove` will return `None` and `unwrap()` will panic.

**Severity:** Medium (Brittle design, crash on future modification or logic bug)

**Suggested Fix:** Rather than using `unwrap()`, handle the `Option` safely: `if let Some(id_to_evict) = self.lru_queue.remove(i) { ... }`. Alternatively, avoid removing by index inside a loop entirely; track nodes to remove and remove them by ID, or pop from the back of the deque safely.
