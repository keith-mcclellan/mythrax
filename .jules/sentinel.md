## 2024-05-31 - [Mutex Poisoning DoS Vulnerability]
**Vulnerability:** The `SizeRollingFileWriter` used `.unwrap()` when locking its internal state mutex (`self.inner.lock().unwrap()`).
**Learning:** If a thread panicked while holding the logger's mutex, the mutex would become "poisoned", and any subsequent attempts to log from other threads would also panic when calling `.unwrap()`. This cascades the failure, potentially causing a Denial of Service (DoS) and failing to log the very errors needed for debugging.
**Prevention:** Always use `.unwrap_or_else(|e| e.into_inner())` or a `match` statement when locking a mutex in a critical path like logging to ensure the application remains resilient and continues operating even if a thread panics.
