---
title: Deadlock risk due to multiple fine-grained Mutexes in LLM manager
labels: bug, agent-found
---

**File:** `mythrax-core/src/llm/mod.rs`
**Lines:** 1587, 1589, 1835, 1839, 1868, etc.

**Bug Type:** Deadlock / Race Condition Risk

**Description:**
The LLM manager makes heavy use of fine-grained `std::sync::Mutex` (e.g., `self.models.lock().unwrap()`, `self.last_weak_ref.lock().unwrap()`, `self.active_tier.lock().unwrap()`).
In several places, it acquires these locks sequentially or while holding another lock.
For example, in `acquire_llm` around line 1584:
```rust
let mut models = self.models.lock().unwrap();
if let Some(model) = models.get(&tier) {
    let mut last_weak_ref = self.last_weak_ref.lock().unwrap();
    // ...
    let mut active_tier = self.active_tier.lock().unwrap();
```
If another thread acquires `active_tier` then tries to acquire `models`, a classic deadlock will occur.

**Minimal Reproducible Scenario:**
Run multiple concurrent requests that trigger model eviction and acquisition simultaneously. The arbitrary locking order across different functions will eventually interleave and deadlock the async executor.

**Severity:** High (Application Hang / Deadlock)

**Suggested Fix:**
Combine these highly coupled state variables into a single `Mutex<LlmState>` struct so they are always locked and updated atomically, eliminating the possibility of lock inversion deadlocks.
