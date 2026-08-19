---
title: "Bug: Potential race condition and integer overflow in Executor port allocation"
labels: ["bug", "agent-found"]
---

**File:** `mythrax-core/src/cognitive/executor.rs`
**Line:** 201 (`let port_offset = (node_id.chars().map(|c| c as u32).sum::<u32>() % 1000) as u16;`)

**Minimal Reproducible Scenario:**
In `ArborExecutor::execute`, API and DB ports are allocated dynamically based on a hash of the `node_id`. Since `node_id` strings can hash to the same value mod 1000, two concurrent executions might be assigned the exact same API and DB ports (e.g., if their sum mod 1000 is identical). This leads to a "port already in use" error and breaks test isolation.

**Severity:** Medium (Flaky test failures due to port collision in concurrent async executor tasks).

**Suggested Fix:**
Instead of a simple character sum mod 1000, use an atomic counter or assign a free port provided by the OS (e.g. binding to port 0 to let the OS assign, then passing the assigned port to the child process, or locking a global port registry).
