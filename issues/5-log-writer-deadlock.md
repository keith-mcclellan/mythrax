---
title: Potential deadlock / blocking operation in LogWriter
labels: ["bug", "agent-found"]
---
## Bug Description
**File and Line Number:** `mythrax-core/src/cli/log_writer.rs` around line 34, 100, 115, and `mythrax-core/src/main.rs:1663`

**Minimal Reproducible Scenario:** The codebase uses `std::sync::Mutex` (`self.inner.lock().unwrap()`) to synchronize writing to log files or the CLI writer. Since this is an async application running under Tokio (as evident from `#[tokio::main]`), using a synchronous `std::sync::Mutex` across potentially blocking I/O operations (`file.flush()?`, `self.write_all_and_roll(buf)?`) can block the Tokio worker threads. If a blocking filesystem operation takes too long, it will starve the async executor and cause deadlocks or latency spikes.

**Severity:** Medium (Performance degradation or potential executor starvation)

**Suggested Fix:** Use `tokio::sync::Mutex` if holding the lock across `.await` points, or use `tokio::task::spawn_blocking` for blocking filesystem I/O. For logging, consider using dedicated non-blocking asynchronous log writers like `tracing-appender`.
