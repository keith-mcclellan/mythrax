---
title: Mutex poisoning panic in log_writer.rs
labels: bug, agent-found
---

**File:** `mythrax-core/src/cli/log_writer.rs`
**Line Numbers:** 34, 100, 115

**Description:**
The code uses `self.inner.lock().unwrap()`. A mutex poisoning panic path exists where this will panic and crash the daemon if another thread previously panicked while holding the lock.

**Minimal Reproducible Scenario:**
Trigger a panic in a thread while it holds the `log_writer` lock. Subsequently, attempt to write to the log from another thread. The daemon will crash completely due to the `unwrap()` on the poisoned mutex.

**Severity:** Critical

**Suggested Fix:**
Handle the poisoned mutex case explicitly rather than unwrapping, or use a non-poisoning mutex alternative (e.g., from `parking_lot`). For example, replacing `unwrap()` with `.unwrap_or_else(|poisoned| poisoned.into_inner())` if it's safe to continue writing.
