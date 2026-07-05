---
title: "Architecture Review: Evaluation Framework Lacks Adversarial Coverage"
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief

**Finding:** The evaluation framework in `evals/swebench/` exclusively relies on the `SWE-bench_Verified` dataset. It tests only "happy paths" for resolving standard software engineering issues in benign repositories.

**Current Assumption:** The architecture assumes that an agent's capability to solve standard GitHub issues is an adequate proxy for production readiness, and that security or resilience testing is either unnecessary or implicitly covered by functional correctness.

**Attack Scenario:** Because the system is never evaluated against adversarial inputs, an attacker crafts a repository specifically designed to break the agent. This repo could contain symlink bombs, recursive directory structures, or files with prompt-injection payloads in the `README.md`. When the agent is tasked to analyze this repository, the unbounded recursion exhausts daemon resources, or the injected prompt hijacks the agent's control flow, resulting in catastrophic failure or exploitation.

**Blast Radius:** The deployment of architecturally dishonest systems into production. By relying solely on happy-path evaluations, Mythrax will fail unpredictably when exposed to real-world, messy, or actively hostile codebases, severely damaging user trust and potentially compromising the host system.

**Recommended Structural Change:**
- Integrate adversarial evaluation datasets into the `evals/` harness.
- Test for unbounded recursion, symlink attacks, and prompt injection resilience explicitly.
- The evaluation score must heavily penalize failures in these adversarial scenarios, treating security and degradation boundaries as first-class citizens alongside functional SWE-bench resolution rates.

*ADR required to close this issue.*