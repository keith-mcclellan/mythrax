---
title: "Evals Framework Exhibits Severe Happy-Path Bias"
tags: [architecture-review, adversarial]
---

# Finding: Evals Framework Exhibits Severe Happy-Path Bias

## Current Assumption
The `evals/swebench/` framework accurately measures the cognitive and structural resilience of the agent and daemon by evaluating standard SWE-bench Verified python bug fixes.

## Attack Scenario
The system is deployed into an environment with adversarial inputs (e.g., a PR containing prompt injection, an unexpectedly large file exceeding memory limits, or a malformed JSON payload). The agent fails catastrophically because the evals framework only tests predictable, well-formed "happy path" scenarios.

## Blast Radius
False Sense of Security / Silent Production Failures. The architecture is deemed "verified" while remaining vulnerable to unbounded recursion, token exhaustion, and context window poisoning.

## Recommended Structural Change
Introduce a mandatory Adversarial Evals suite (e.g., "SWE-bench-Poisoned"). Inject prompt injections into the test issues, simulate network timeouts during model retrieval, and feed malformed syntax to the pre-compaction hooks to verify structural resilience and failure containment.

*This issue requires an ADR response to close.*
