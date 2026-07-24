---
title: "🛡️ Sentinel: [HIGH] Evaluation Framework Lacks Adversarial Testing"
labels: ["architecture-review", "adversarial", "bug", "agent-found"]
---

# Vulnerability Report: Incomplete Evaluation Coverage

## Finding
The evaluation framework (`evals/swebench/eval.sh`) relies solely on the SWE-bench Verified dataset for performance scoring. It completely lacks adversarial input, prompt injection, or resource exhaustion testing.

## Current Assumption
High performance on functional benchmarks (happy paths) directly correlates with robustness and safety in real-world, potentially hostile environments.

## Attack Scenario
The system is deployed based on high SWE-bench scores but fails catastrophically when encountering malformed, ambiguous, or malicious inputs in production, as these paths were never tested or penalized during the development cycle.

## Blast Radius
A false sense of security leading to deployment in sensitive environments where the system can be trivially manipulated, crashed, or compromised by adversarial inputs.

## Recommended Structural Change
Integrate dedicated adversarial datasets (e.g., prompt injection suites, malformed ASTs, resource exhaustion payloads) into the `evals/` framework. The scoring system must evaluate and heavily penalize failures on adversarial inputs with the same rigor as functional failures to ensure true architectural robustness.

---
*Note: Do not close this issue without a documented Architectural Decision Record (ADR) response.*