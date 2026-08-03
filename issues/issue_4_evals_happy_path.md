---
title: "Adversarial CTO: Deceptive Evals and Lack of Adversarial Input Testing"
labels: ['architecture-review', 'adversarial']
---

## Finding
Deceptive Evals: Happy-Path Only Benchmarking in `evals/swebench/`

## Current Assumption
High scores on the SWE-bench Verified coding benchmark accurately reflect the agent's robustness, architectural readiness, and safety.

## Attack Scenario
The system encounters an adversarial, malformed, or intentionally deceptive input in production (e.g., recursive zip bombs, obfuscated prompt injections, or manipulated codebase structures). Because the eval framework only tests verified, functional "happy paths," the agent processes these inputs naively, bypassing scope boundaries and failing to fail gracefully. LLM-based systems that only test happy paths are architecturally dishonest.

## Blast Radius
System brittleness, unexpected catastrophic failures, and vulnerability to non-cooperative inputs, completely undermining the functional assurances provided by the benchmarks.

## Recommended Structural Change
Integrate adversarial red-teaming benchmarks, chaos engineering, boundary testing, and malformed input fuzzing directly into the `evals/` suite. Ensure evaluation explicitly measures failure handling and scope boundary enforcement. Never close this issue without a documented ADR response.
