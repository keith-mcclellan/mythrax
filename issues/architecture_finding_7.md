---
title: "Eval Framework Adversarial Dishonesty: SWE-bench Happy Path Bias"
labels: ["architecture-review", "adversarial"]
---

## Finding
The eval framework in `evals/swebench/eval.sh` relies purely on the standard SWE-bench dataset.

## Current Assumption
Standard dataset evaluation (SWE-bench) is sufficient to validate agent correctness and safety.

## Attack Scenario
The eval framework only tests for happy-path code resolution. It does not inject adversarial inputs, prompt injections, or malicious dependencies into the environment. An adversarial user or compromised dependency can exploit prompt injection or unbounded recursion vulnerabilities, which the eval framework fails to detect.

## Blast Radius
False confidence in system security. Deployment of an easily manipulable autonomous agent vulnerable to malicious instructions and unbounded resource consumption.

## Recommended Structural Change
Introduce a dedicated adversarial evaluation suite (e.g., 'Red-Bench') that specifically tests prompt injection resilience, scope boundary enforcement, and unbounded recursion limits alongside standard correctness tests.

> Note: This issue requires a documented architectural decision record (ADR) response before it can be closed.
