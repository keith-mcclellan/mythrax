---
title: Logic Bug: Potential panic due to missing values during cognitive metric arithmetic
labels: bug, agent-found
---

**File & Line:** `mythrax-core/src/api.rs` (Various parsing of JSON bodies where integers/metrics are involved, e.g. `unwrap()` on parsing integers)

**Minimal Reproducible Scenario:** In some edge cases within agent metric/score calculations, division by zero results in NaN for floats, or integer math over/underflows, leading to silent metric corruption.

**Severity:** Medium

**Suggested Fix:** Explicitly check divisors for zero, use `saturating_add` / `saturating_sub` for unsigned integer arithmetic involving scores or agent caps.