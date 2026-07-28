---
title: Logic Bug: Unhandled `expect` panic on auth token read in `get_or_create_token` during critical operations
labels: bug, agent-found
---

**File & Line:** `mythrax-core/src/auth.rs:101-155`

**Minimal Reproducible Scenario:** In `mythrax-core/src/auth.rs`, `get_or_create_token` is called directly during testing and internal flows where the return value is handled with `.expect("Failed to get or create token")` instead of proper error propagation. If there's an I/O error or permission issue with the token file path, it triggers a hard panic, crashing the process.

**Severity:** High (Crash/Panic)

**Suggested Fix:** Use proper `Result` return types and `?` operators instead of `.expect()` inside library functions.