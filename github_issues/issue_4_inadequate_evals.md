---
title: "Architecture Review: Inadequate Evaluation Framework – Testing Only the Happy Path"
labels: ["architecture-review", "adversarial"]
---

### Finding
Inadequate Evaluation Framework – Testing Only the Happy Path

### Current Assumption
Utilizing the official SWE-bench Verified harness in `evals/swebench/eval.sh` accurately measures and guarantees the agent's coding capability and robustness.

### Attack Scenario
The official SWE-bench harness evaluates only whether a model can resolve a specific, well-defined bug in a sterile environment. It explicitly *does not* test resilience against poisoned codebases, ambiguous conflicting constraints, or adversarial inputs. LLM-based systems evaluated solely on happy-path completion are architecturally dishonest. An attacker submitting a PR with benign-looking but adversarial syntax will easily bypass the agent, which over-indexes on typical patterns because it was never evaluated under adversarial conditions.

### Blast Radius
A system certified as "highly capable" but structurally brittle in production. Agents easily confused by edge cases or manipulated by codebase context.

### Recommended Structural Change
Expand the `evals/` framework to include a dedicated Red Team test suite. Introduce evaluations that intentionally subvert agent instructions, provide infinite-looping codebase structures, and supply conflicting architectural constraints to test boundary enforcement and failure recovery.

> **Note:** Do not close this issue without a documented Architectural Decision Record (ADR) response.