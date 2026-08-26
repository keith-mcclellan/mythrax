---
labels: bug, agent-found
severity: High
---

# Mutex poisoning panic path in Log Writer

**File:** `mythrax-core/src/cli/log_writer.rs`
**Line:** 34, 100, 115

## Description
A panic path exists where `self.inner.lock().unwrap()` will panic and crash the daemon if another thread previously panicked while holding the lock (Mutex poisoning).

## Minimal Reproducible Scenario
1. Thread A acquires the log writer lock.
2. Thread A encounters an unexpected panic while holding the lock (e.g., due to an OOM or unexpected signal).
3. The mutex is now poisoned.
4. Thread B attempts to write a log, calling `self.inner.lock().unwrap()`.
5. Thread B panics due to the poisoned mutex, causing a cascading failure that crashes the entire daemon.

## Suggested Fix
Replace `.unwrap()` with robust error handling, either by propagating the error, logging the failure to standard error and continuing, or by intentionally handling the `PoisonError` (e.g., by recovering the lock or resetting the writer). For example: `self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())`.
