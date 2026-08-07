---
title: Off-by-one / Logic error in context window loops for ConvergenceDetector
labels: bug, agent-found
---

**File:** `mythrax-core/src/cognitive/arbor.rs`
**Line:** 181

**Description:**
In `ConvergenceDetector::record_score()`, the logic checks `if self.history.len() < self.window_size { return ConvergenceSignal::Converging; }`. It then computes `delta_visits = (self.history.len() - 1) as f32;`. If `window_size` is initialized to `.max(2)`, and exactly 2 elements are in history, `delta_visits` evaluates to `1.0`. The early return logic based on `window_size` points may prematurely evaluate convergence gradients on an insufficient number of intervals, leading to false convergence signals in the cognitive loop.

**Minimal Reproducible Scenario:**
Instantiate `ConvergenceDetector::new(2)`. Call `record_score(0.5)`. `history.len()` is 1, returns `Converging`. Call `record_score(0.8)`. `history.len()` is 2. `delta_visits` is 1.0. Calculates `score_velocity = 0.3`. If `window_size` was intended to represent the minimum number of intervals required for a stable gradient, the current logic calculates velocity prematurely on a single interval.

**Severity:** Low (Correctness)

**Suggested Fix:**
Clarify whether `window_size` refers to points or intervals, and ensure the early return logic correctly aligns with the mathematical intention, perhaps requiring `self.history.len() > self.window_size` intervals before evaluating velocity.
