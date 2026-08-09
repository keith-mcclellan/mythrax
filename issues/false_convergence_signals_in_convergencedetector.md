---
title: False convergence signals in ConvergenceDetector
labels: bug, agent-found
---

**File:** `mythrax-core/src/cognitive/arbor.rs`
**Line:** 181-183 (early return logic)

**Description:**
`ConvergenceDetector`'s early return logic based on `window_size` limits can prematurely trigger false convergence signals by evaluating gradients on an insufficient number of intervals. For example, if the history length is very small (e.g., yielding `delta_visits = 1.0`), returning a concrete convergence signal is mathematically unstable and conceptually flawed.

**Minimal Reproducible Scenario:**
Initialize a `ConvergenceDetector` with a very small window size or call `record_score` once/twice. The system immediately outputs `ConvergenceSignal::Converging` before a statistically significant trajectory can be established, potentially short-circuiting the AI agent's ideation or evaluation loops prematurely.

**Severity:** Medium

**Suggested Fix:**
Enforce a higher minimum window size and introduce an `InsufficientData` or `Pending` state rather than defaulting to `Converging` when `history.len() < window_size`.