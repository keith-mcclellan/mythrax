# Bug: Mutex poisoning panic path in LogWriter crashes daemon

**File and Line Number**: `mythrax-core/src/cli/log_writer.rs`, lines 34, 100, 115

**Minimal Reproducible Scenario**:
If a thread panics while holding the `inner` lock during a log write operation, the Mutex becomes poisoned. Any subsequent thread calling `self.inner.lock().unwrap()` (e.g., during `flush()` or `write_all_and_roll()`) will instantly panic, cascading the failure and crashing the entire Mythrax daemon.

**Severity**: High

**Suggested Fix**:
Replace `.lock().unwrap()` with `.lock().unwrap_or_else(|poisoned| poisoned.into_inner())` to recover the lock state and continue logging, or gracefully bubble up the error instead of panicking.

**Labels**: `bug`, `agent-found`