---
title: "Bug: Panic Path via Float Sorting in Synthesis Embeddings"
labels: ["bug", "agent-found"]
---

**File:** `mythrax-core/src/cognitive/synthesis.rs`
**Line:** ~631, ~636

**Description:**
The distance between embeddings is calculated using `cosine_distance` and the results are sorted. The code uses `dists.sort_by(|a, b| a.partial_cmp(b).unwrap());`. If a calculation results in `NaN` (e.g. from a zero-length vector leading to division by zero), `partial_cmp` returns `None`. Calling `.unwrap()` on `None` causes the application to panic, creating an unreliable edge case during embedding operations.

**Minimal Reproducible Scenario:**
Processing embeddings that evaluate to `NaN` during `cosine_distance` causes a crash when `sort_by` calls `.unwrap()`.

**Severity:**
High (Crash)

**Suggested Fix:**
Replace `.unwrap()` with `.unwrap_or(std::cmp::Ordering::Equal)` to safely handle `NaN` cases.
