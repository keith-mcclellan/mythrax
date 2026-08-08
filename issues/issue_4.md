# Bug: Early convergence via gradient logic in ConvergenceDetector
**Labels**: bug, agent-found

**File**: `mythrax-core/src/cognitive/arbor.rs`
**Line**: 188
**Severity**: Medium

**Scenario**:
`ConvergenceDetector` evaluates early return logic based on `window_size` limits. It can prematurely trigger false convergence signals by evaluating gradients on an insufficient number of intervals (e.g., `delta_visits = (self.history.len() - 1) as f32`, where `delta_visits` can be small or 1.0 depending on window size and history), falsely halting optimization routines.

**Suggested Fix**:
Ensure minimum history size constraints before evaluating velocity gradients and convergence.