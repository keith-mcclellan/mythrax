---
title: "Eval Framework Gap: Dishonest Eval Framework Lacks Adversarial Inputs"
labels: ["architecture-review", "adversarial"]
---

# Finding: Dishonest Eval Framework (Happy-Path Bias)

## Current Assumption
The SWE-bench Verified harness in `evals/swebench` adequately measures the system's cognitive performance and robustness.

## Attack Scenario
The system performs well on happy-path PR generation but fails catastrophically when fed poisoned data. The eval framework (eval.sh / summarize.py) does not test how the agent handles contradictory instructions, prompt injection embedded in target codebases, or infinite loops triggered by adversarial file names.

## Blast Radius
False confidence in system robustness. Deployment to production environments where adversarial code triggers unbounded token generation, infinite loops, or catastrophic failure.

## Recommended Structural Change
Introduce a dedicated `evals/adversarial` test suite featuring prompt-injected codebases, self-contradicting requirements, and infinite recursion traps to validate true system resilience.

**Status:** Requires Architectural Decision Record (ADR) response to close.