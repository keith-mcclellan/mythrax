---
title: Test coverage gap: math::cosine_similarity
labels: bug, agent-found
---

**File:** `mythrax-core/src/math.rs`
**Line Number:** 9

**Description:**
The public function `pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32` has no corresponding tests in the test suite or evals.

**Minimal Reproducible Scenario:**
Run test coverage analysis (`cargo tarpaulin`). The `cosine_similarity` function will have 0% coverage.

**Severity:** Medium

**Suggested Fix:**
Add unit tests for `cosine_similarity` to ensure its correctness across different vector inputs and edge cases (e.g., zero vectors).
