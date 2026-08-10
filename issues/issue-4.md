---
title: "Adversarial Architecture Review: Inadequate Eval Framework"
labels: architecture-review, adversarial
---

**Finding:** The evaluation framework in `evals/swebench/` exclusively uses standard SWE-bench instances (happy path logic/features).
**Current Assumption:** High performance on SWE-bench correlates to robustness in real-world deployment.
**Attack Scenario:** The agent is exposed to a repository with deliberately obfuscated malicious instructions in `AGENTS.md` or a prompt injection in a test case. Since the eval harness never stresses the agent's refusal logic or scope boundaries, the agent blindly executes the malicious instructions, potentially escaping its worktree or compromising the system.
**Blast Radius:** False confidence in agent safety, leading to deployment in high-trust environments where adversarial inputs result in immediate compromise. LLM-based systems that only test happy paths are architecturally dishonest.
**Recommended Structural Change:** Integrate adversarial evaluation suites (e.g., prompt injection benchmarks, out-of-bounds action detection) into the `evals/` framework. Mandate that every PR modifying orchestration or memory pipelines passes an adversarial resistance test.

Never close without a documented architectural decision record (ADR) response.
