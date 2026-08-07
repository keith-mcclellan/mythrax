---
title: Integer overflow risk in access_count for metrics
labels: bug, agent-found
---

**File:** `mythrax-core/src/db/crud_operations.rs`
**Line:** 2919

**Description:**
The code increments `access_count` directly using `let new_count = row.access_count + 1;`. If `access_count` reaches the maximum value of its integer type, this will panic in debug mode (or with overflow checks enabled) or wrap around to 0 in release mode. This can heavily distort the `utility_score` calculation which relies on `new_count` to rank agent memory nodes.

**Minimal Reproducible Scenario:**
Artificially set the `access_count` of a `memory_metrics` record to `i32::MAX` (or whatever the underlying type's max is) in SurrealDB. Trigger a memory access to that node. The resulting `new_count = row.access_count + 1` will panic or overflow, crashing the daemon or resetting the utility score unexpectedly.

**Severity:** Medium (Correctness / Crash)

**Suggested Fix:**
Use `saturating_add(1)` to prevent overflow:
```rust
let new_count = row.access_count.saturating_add(1);
```
