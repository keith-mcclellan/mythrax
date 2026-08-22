## Bug Report: `log_writer.rs` Mutex Poisoning Panic Path

**File & Line:** `mythrax-core/src/cli/log_writer.rs`, lines 34, 100, 115 (e.g., `let mut inner = self.inner.lock().unwrap();`)

**Minimal Reproducible Scenario:** If another thread panics while holding the `inner` lock in `RollingFileLogger`, the Mutex becomes poisoned. When any subsequent log message is written, `self.inner.lock().unwrap()` panics, crashing the daemon.

**Severity:** High (Crash in production daemon).

**Suggested Fix:** Use `.unwrap_or_else(|e| e.into_inner())` to recover the lock or replace `std::sync::Mutex` with `parking_lot::Mutex`.
