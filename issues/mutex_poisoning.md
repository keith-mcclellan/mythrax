Title: Mutex poisoning panic path in SizeRollingFileWriter
Labels: bug, agent-found

File: mythrax-core/src/cli/log_writer.rs, Lines: 34, 100, 115
Severity: High
Scenario: A thread panicked while holding `self.inner.lock()`, poisoning the mutex. A subsequent thread calls `.write()` or `.flush()` and calls `self.inner.lock().unwrap()`, which panics and crashes the daemon.
Suggested Fix: Use `.lock().unwrap_or_else(|e| e.into_inner())` or handle the `PoisonError` gracefully to avoid crashing the daemon.
