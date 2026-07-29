---
title: Daemon crashes completely if SIGTERM/SIGINT registration fails
labels: bug, agent-found
severity: High
---

**File/Line:** `mythrax-core/src/daemon.rs` : 597

**Minimal Reproducible Scenario:**
If the process fails to register a signal handler (e.g. process limits or OS issues) via `tokio::signal::unix::signal`, the `expect("Failed to register SIGTERM handler")` will panic, crashing the entire daemon immediately upon startup instead of gracefully degrading or bubbling up the error.

**Suggested Fix:**
Return the error instead of panicking:
```rust
let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
let mut sigint = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
```
