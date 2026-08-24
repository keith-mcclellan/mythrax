**Title:** Mutex poisoning panic path in Log Writer
**Labels:** bug, agent-found
**File:** mythrax-core/src/cli/log_writer.rs
**Line:** 34, 100, 115
**Severity:** High
**Minimal Reproducible Scenario:** A thread panics while holding the log writer lock. Another thread attempts to acquire the lock and calls `.unwrap()`, which panics due to PoisonError, crashing the daemon.
**Suggested Fix:** Handle `PoisonError` gracefully or use a lock-free structure.
