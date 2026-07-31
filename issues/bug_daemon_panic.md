# Bug: Daemon process panics on signal handler registration failure

**Labels:** bug, agent-found

**File/Line:** `mythrax-core/src/daemon.rs`, lines 597 and 601

**Minimal Reproducible Scenario:**
If the application is run in an environment where signal registration fails (e.g. some restricted containers or nested process groups), the `unwrap()` or `expect()` calls on `tokio::signal::unix::signal` will cause the entire daemon to crash immediately on startup, failing to gracefully handle or propagate the error.

**Severity:** High

**Suggested Fix:**
Replace `.expect("Failed to register SIGTERM handler")` and `.expect("Failed to register SIGINT handler")` with `?` error propagation to safely surface the error and allow the daemon or caller to handle the failure gracefully without panicking.