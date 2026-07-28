---
title: Logic Bug: Floating point comparison unwrap panic on NaN
labels: bug, agent-found
---

**File & Line:** `mythrax-core/src/cognitive/synthesis.rs:996` and `1001`

**Minimal Reproducible Scenario:** In the clustering / synthesis logic, distances are sorted using `dists.sort_by(|a, b| a.partial_cmp(b).unwrap())`. If any distance is `NaN` (which can happen if a vector is zero-length and cosine similarity does a division by zero, or if the input is malformed), `partial_cmp` returns `None` and `unwrap()` panics.

**Severity:** High (Crash/Panic)

**Suggested Fix:** Use `.unwrap_or(std::cmp::Ordering::Equal)` instead of `.unwrap()`.