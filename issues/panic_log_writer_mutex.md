---
title: Panic on Mutex lock in log_writer
labels: bug, agent-found
---
# Bug Report

**Severity:** HIGH
**File:** `mythrax-core/src/cli/log_writer.rs`
**Line:** 34, 100, 115

## Minimal Reproducible Scenario
The lines `let mut inner = self.inner.lock().unwrap();` inside `write_all_and_roll` and `flush` methods will panic if the mutex gets poisoned. If any thread holding this lock panics (e.g., due to an I/O error unwrap that might exist elsewhere), subsequent calls to log anything will panic, potentially cascading the failure across the entire application since logging is a core utility used everywhere.

## Suggested Fix
Use `.unwrap_or_else(|e| e.into_inner())` to recover the poisoned mutex if appropriate, or handle the `PoisonError` gracefully to avoid crashing the logging system entirely.
