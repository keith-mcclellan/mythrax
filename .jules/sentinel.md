## 2024-05-18 - Prevent DoS via Mutex Poisoning
**Vulnerability:** Mutex lock acquisitions using `.unwrap()` can panic if the mutex is poisoned, leading to a cascade of panics and potential Denial of Service (DoS).
**Learning:** In Rust, if a thread panics while holding a mutex, the mutex becomes poisoned. Calling `.unwrap()` on the `Result` returned by `lock()` will cause the calling thread to panic as well.
**Prevention:** Always handle mutex locks gracefully using `.unwrap_or_else(|e| e.into_inner())` or a match statement to recover the guard even if the mutex is poisoned.
