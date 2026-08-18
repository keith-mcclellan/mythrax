---
labels: architecture-review, adversarial
state: open
---
# Red Team Architecture Brief: Unbounded Recursion Risk in Arbor HTR Verification

**Finding** -> The Arbor Hypothesis-Tree Refinement (HTR) Parallel Verification Loop lacks global bounding for runaway subagent branching.
**Current Assumption** -> Agents will naturally converge and resolve issues within the localized `max_steps` per task.
**Attack Scenario** -> A subagent encounters a flaky test or ambiguous error message and enters an infinite loop of generating hypotheses, testing them in isolated git worktrees, and failing, spanning out continuously.
**Blast Radius** -> File descriptor exhaustion, CPU pinning, and disk space exhaustion from unbounded temporary git worktrees (`tmp/`).
**Recommended Structural Change** -> Enforce a global depth limit and a compute/token budget per HTR root task. Implement exponential backoff and circuit breakers for repeated verification failures. Must require an ADR response to close.
