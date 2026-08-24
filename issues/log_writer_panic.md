---
title: Mutex poisoning panic in SizeRollingFileWriter
labels: bug, agent-found
---
**File and Line:** `mythrax-core/src/cli/log_writer.rs`, lines 34, 100, 115

**Minimal Reproducible Scenario:** If a thread panics while holding the `SizeRollingFileWriterInner` lock (for instance, due to an unhandled error inside the thread), the mutex becomes poisoned. Subsequent calls to `.write()` or `.flush()` from other threads will hit `self.inner.lock().unwrap()` and immediately panic, causing the entire daemon to crash.

**Severity:** High

**Suggested Fix:** Use `.lock().unwrap_or_else(|poisoned| poisoned.into_inner())` to recover the lock, or properly return an `io::Error` rather than panicking on a poisoned mutex.
