---
title: Panic setting signal handler
labels: bug, agent-found
---

**File:** `mythrax-core/src/daemon.rs`
**Lines:** 336, 340

**Scenario:** `.expect()` is used during setup when registering SIGTERM and SIGINT handlers. If registering these handlers fail, the daemon crashes.

**Severity:** Medium

**Suggested Fix:** Use `?` operator instead of `.expect()`.
