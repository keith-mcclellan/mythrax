# [MEDIUM] Eval Framework Lacks Adversarial Honesty

**Labels:** `architecture-review`, `adversarial`, `testing`

## Finding
The current SWE-bench evaluation framework (`evals/swebench/eval.sh`) relies entirely on the SWE-bench Verified dataset for performance scoring.

## Current Assumption
The SWE-bench Verified framework accurately measures the system's performance, reliability, and robustness.

## Attack Scenario
The system only optimizes for "happy path" code resolutions in a sterile environment. It does not test how the model behaves when given conflicting instructions, hallucinated test files, maliciously crafted PRs, or prompt injection attempts.

## Blast Radius
The system may achieve high benchmark scores but will silently fail or be easily compromised when deployed to real-world, messy, or adversarial codebases.

## Recommended Structural Change
Augment the eval harness with an adversarial dataset (e.g., PromptMap, Garak, or custom perturbed SWE-bench issues with hidden prompt injections) to explicitly measure resilience and boundary enforcement.

**Note:** Do not close this issue without a documented Architectural Decision Record (ADR) response.
