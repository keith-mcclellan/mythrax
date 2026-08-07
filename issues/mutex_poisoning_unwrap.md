---
title: Panic vulnerabilities via .unwrap() on Mutex::lock() across mythrax-core
labels: bug, agent-found
---

**File:** `mythrax-core/src/llm/mod.rs` (and other files)
**Lines:** e.g., 350, 1501, 1513, etc.

**Minimal Reproducible Scenario:**
Many structs in `mythrax-core` (such as the `ModelBroker` in `llm/mod.rs` and the watcher in `vault/watcher.rs`) use `std::sync::Mutex` and call `.lock().unwrap()`. In Rust, a Mutex becomes poisoned if a thread panics while holding the lock. If an inner thread panics during an LLM inference request or a vault watcher operation, any subsequent request that attempts to acquire the lock will call `.unwrap()` on the resulting `Err`, causing a secondary panic and crashing the entire daemon process.
For example, if `broker.acquire_llm()` panics inside while holding `models`, the next request to `acquire_llm()` will panic at `let mut models = self.models.lock().unwrap();`.

**Severity:** Medium (can cause cascading failures and denial of service across concurrent threads).

**Suggested Fix:**
Instead of unwrapping `Mutex::lock()`, gracefully recover from the poisoned state if possible, or use `.unwrap_or_else(|e| e.into_inner())` if it's safe to use the potentially inconsistent data, or bubble up an error using `Result`.

```rust
let mut models = match self.models.lock() {
    Ok(guard) => guard,
    Err(poisoned) => {
        tracing::warn!("Models mutex was poisoned, recovering state...");
        poisoned.into_inner()
    }
};
```
