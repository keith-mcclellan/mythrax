---
title: Premature Convergence Signal in Arbor HTR
labels: bug, agent-found
---

**Bug Type:** Logic Branch (False Positive)
**File:** `mythrax-core/src/cognitive/arbor.rs`
**Line:** 181-183
**Severity:** High
**Minimal Reproducible Scenario (Trigger):**
A `ConvergenceDetector` is initialized with `window_size = 5`. Due to early return logic based on `window_size` limits (`if self.history.len() < self.window_size { return ConvergenceSignal::Converging; }`), it prematurely triggers false convergence signals by evaluating gradients on an insufficient number of intervals before enough data is collected to accurately assess the trend. This causes the HTR optimization loop to stop prematurely with an unrefined model.

**Suggested Fix:**
Add a new enum variant `NotConverged` or `Gathering` to `ConvergenceSignal` and return it instead of `Converging` when `self.history.len() < self.window_size` and when `delta_visits <= 0.0`.
