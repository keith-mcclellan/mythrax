---
title: "Red Team Architecture Brief: Evaluation Framework Lacks Adversarial Robustness Testing"
labels: ["architecture-review", "adversarial"]
---

**Finding:** The evaluation framework in `evals/` (specifically the SWE-bench wrapper) focuses exclusively on verifying expected functionality and correct patch application (happy paths).

**Current Assumption:** The assumption is that if an LLM-based agent can successfully resolve coding issues and pass standard test suites, the underlying cognitive architecture is sound and robust.

**Attack Scenario:** The system is deployed to handle untrusted or adversarial inputs (e.g., parsing untrusted logs, handling external PRs). Because the evals only test cooperative scenarios, they fail to detect vulnerabilities such as prompt injection, denial-of-service via massive context windows, or unbounded recursion triggered by cyclic data. An attacker exploits these untested edge cases to disrupt the agent, corrupt memory, or cause system crashes. LLM-based systems that only test happy paths are architecturally dishonest, as they ignore the probabilistic and easily manipulable nature of language models.

**Blast Radius:** Unquantified risk across the entire system. Without adversarial evals, the true resilience of the cognitive pipeline, memory retrieval, and agent orchestration against malicious manipulation is unknown, potentially leading to systemic compromise in production environments.

**Recommended Structural Change:**
1. Expand the `evals/` framework to include explicit adversarial test suites (Red Teaming evals) that attempt prompt injections, resource exhaustion (e.g., passing 100k token files), and logic manipulation.
2. Implement automated fuzzing for the API gateway and MCP tool endpoints to ensure graceful degradation and error handling under malformed or malicious inputs.
3. Establish a baseline for adversarial robustness metrics (e.g., successful injection rejection rate) that must be passed before merging changes to `mythrax-core/src/`.
