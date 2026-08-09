---
title: Integer overflow risk in metrics access_count
labels: bug, agent-found
---

**File:** `mythrax-core/src/db/crud_operations.rs`
**Line:** 2919 (`let new_count = row.access_count + 1;`)

**Description:**
The metric `access_count` is updated by adding 1 without overflow checks. In Rust, this can cause a panic in debug mode or if overflow checks are enabled, and silently wrap around to negative numbers otherwise.

**Minimal Reproducible Scenario:**
If a specific `target_id` is queried a huge number of times (e.g., nearing `i64::MAX`), fetching its metrics and evaluating `row.access_count + 1` will overflow. This causes the API to crash (DoS) or corrupt the `utility_score` calculation.

**Severity:** High

**Suggested Fix:**
Use `.saturating_add(1)` instead of the `+ 1` operator:
```rust
let new_count = row.access_count.saturating_add(1);
```