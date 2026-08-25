---
labels: bug, agent-found
---

**File:** `mythrax-core/src/cli/log_writer.rs`
**Line Numbers:** 34, 100, 115

**Description:**
The `SizeRollingFileWriter` calls `self.inner.lock().unwrap()` directly. If another thread panics while holding this lock, the mutex becomes poisoned. Subsequent threads attempting to write or flush logs will call `.unwrap()`, triggering a panic and crashing the entire daemon.

**Minimal Reproducible Scenario:**
1. Spawn two threads sharing a `SizeRollingFileWriter`.
2. Thread A acquires the lock and panics deliberately (simulated).
3. Thread B attempts to log, calls `self.inner.lock().unwrap()`, and panics due to PoisonError.

**Severity:** Critical (can crash the daemon unexpectedly)

**Suggested Fix:**
Avoid `.unwrap()`. Handle the `PoisonError` gracefully, for example by acquiring the lock anyway using `.unwrap_or_else(|e| e.into_inner())` or using a resilient locking primitive if strict consistency isn't required for the log file state upon panic.
