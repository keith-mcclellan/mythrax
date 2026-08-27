## Bug Report: Mutex poisoning panic in log_writer.rs

**File:** `mythrax-core/src/cli/log_writer.rs`
**Line:** 34, 100, 115

**Minimal Reproducible Scenario:**
If another thread panics while holding the lock on `self.inner`, the Mutex becomes poisoned. When another thread attempts to acquire the lock using `self.inner.lock().unwrap()`, it will immediately panic and crash the daemon. This is particularly dangerous for logging mechanisms since they are shared across threads, and a panic in one component shouldn't necessarily take down the whole daemon via the logger.

**Severity:** High (Crash/Denial of Service)

**Suggested Fix:**
Handle mutex poisoning gracefully or unwrap to default behavior. For example, use `.unwrap_or_else(|poisoned| poisoned.into_inner())` if continuing despite a previous thread's panic is acceptable, or use a synchronization primitive less prone to poisoning if appropriate.
