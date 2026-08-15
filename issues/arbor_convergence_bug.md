---
labels: bug, agent-found
---

# Logical Flaw in Convergence Detector Metric Velocity

## File and Line
`mythrax-core/src/cognitive/arbor.rs:188`

## Minimal Reproducible Scenario
The `ConvergenceDetector` records sequential scores to detect paradigm shifts or stagnation. It calculates a `score_velocity`:
```rust
let first = self.history.first().copied().unwrap_or(0.0);
let last = self.history.last().copied().unwrap_or(0.0);
let delta_score = last - first;
let delta_visits = (self.history.len() - 1) as f32;
// ...
let score_velocity = delta_score / delta_visits;
```
If the history window is relatively small and elements are pushed until it is full, this logic incorrectly evaluates the overall delta from the *first* to the *last* score divided by `window_size - 1`. If the scores oscillate heavily but the `first` and `last` elements happen to be similar (e.g., `50, 90, 10, 51`), `delta_score` is ~1.0, `score_velocity` is very low, and it evaluates as a `ParadigmShift` or `Stagnant` even though the scores are highly volatile, not stagnant.

This is a classic logic bug where a two-point linear slope is incorrectly used to approximate variance or true velocity over a highly dimensional signal (LLM scoring), causing false positives in stagnation detection and unnecessarily triggering expensive fallback procedures like paradigm shifts.

## Severity
Medium (Causes AI agent inefficiency / thrashing via false positives)

## Suggested Fix
Calculate true variance or average point-to-point absolute delta instead of relying on the simplistic slope between the start and end points of the window.
