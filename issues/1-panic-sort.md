---
title: Panic on sort containing f32::NAN
labels: bug, agent-found
---

**File:** `mythrax-core/src/cognitive/synthesis.rs`
**Lines:** 497, 502

**Scenario:** When calculating distances, a vector of `f32` containing `f32::NAN` elements triggers a panic due to `dists.sort_by(|a, b| a.partial_cmp(b).unwrap())` unwrapping a `None` variant.

**Severity:** High

**Suggested Fix:** Replace `.unwrap()` with `.unwrap_or(std::cmp::Ordering::Equal)`.
