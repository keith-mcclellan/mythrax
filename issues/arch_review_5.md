---
title: "Finding: SWE-Bench Eval Harness Assumes Happy-Path Patch Application"
labels: ["architecture-review", "adversarial"]
---

**Finding** -> SWE-Bench Eval Harness Assumes Happy-Path Patch Application

**Current Assumption** -> The `evals/swebench/eval.sh` harness correctly measures performance by running the official dockerized evaluation against model predictions.

**Attack Scenario** -> The eval framework does not test for prompt injection, context window overflow attacks, or adversarial malformed code snippets. It only tests if a model can solve a known SWE-Bench bug. This gives a false sense of security for production deployment.

**Blast Radius** -> System is deployed believing it is robust, but fails immediately when faced with adversarial user inputs or complex poisoned repositories.

**Recommended Structural Change** -> Integrate adversarial evaluation datasets (e.g., prompt injection suites, massive context overflow tests) into the `evals/` pipeline. Reject PRs that do not pass adversarial evals.
