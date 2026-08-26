## Title: Mutex poisoning panic path in SizeRollingFileWriter


**Labels:** `bug`, `agent-found`

### Description
In `mythrax-core/src/cli/log_writer.rs`, the `write_all_and_roll` and `flush` methods use `self.inner.lock().unwrap()`. If another thread previously panicked while holding this lock, the mutex becomes poisoned. Any subsequent attempt to log will panic the current thread (potentially the main daemon thread) and crash the daemon.

**File:** `mythrax-core/src/cli/log_writer.rs`
**Line:** 30, 83, 93

### Minimal Reproducible Scenario
1. Have multiple threads sharing the `SizeRollingFileWriter`.
2. One thread acquires the lock in `write_all_and_roll`.
3. That thread panics (e.g., due to an unrelated issue or OOM inside the critical section).
4. Another thread attempts to log and calls `self.inner.lock().unwrap()`, which panics with "PoisonError".

### Severity
Critical - Can take down the entire Mythrax daemon due to a single thread's panic.

### Suggested Fix
Handle mutex poisoning gracefully, e.g., by using `.unwrap_or_else(|poisoned| poisoned.into_inner())` or replacing `std::sync::Mutex` with a non-poisoning mutex like `parking_lot::Mutex`.
