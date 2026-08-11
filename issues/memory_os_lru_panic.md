# Bug (agent-found): Panic in `memory_os.rs` LRU eviction via `unwrap()`

**File:** `mythrax-core/src/cognitive/memory_os.rs`
**Line:** 43

**Description:**
In `evict_lru`, the code uses `let id_to_evict = self.lru_queue.remove(i).unwrap();`. Since `self.lru_queue` is modified within the loop during eviction, `remove(i)` can panic if `i` goes out of bounds or the item is missing due to concurrent access or prior removal in the same pass.

**Severity:** High (Crash)

**Minimal Reproducible Scenario:**
If `lru_queue.len()` changes during iteration, or if the `remove` method (which typically expects a valid index or item) fails. This panic path will terminate the agent.

**Suggested Fix:**
Refactor to safely iterate and remove without unwrap. E.g., `if i < self.lru_queue.len() { let id_to_evict = self.lru_queue.remove(i); ... }` and match on Option.
