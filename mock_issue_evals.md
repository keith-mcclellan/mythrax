---
title: "Eval Framework Exhibits Architectural Dishonesty (Happy Path Only)"
labels: ["architecture-review", "adversarial"]
status: "open"
---

## 🛑 Finding: Architectural Dishonesty in Evaluation Framework

**Finding:** The system's evaluation framework (`evals/swebench/eval.sh`) relies entirely on the `SWE-bench_Verified` dataset.

**Current Assumption:** The `SWE-bench_Verified` dataset is assumed to be an adequate measure of an autonomous AI agent's overall safety, reliability, and capability in a real-world environment.

**Attack Scenario:** An agent performs excellently on SWE-bench by successfully applying targeted patches to standard, well-structured bugs (the "happy path"). However, when deployed, the agent encounters malformed inputs, malicious RAG documents containing adversarial instructions, or infinite-loop logic traps. Because the evals framework never tests adversarial inputs or context-window poisonings, the agent fails catastrophically in production while reporting high internal confidence.

**Blast Radius:** Systemic overconfidence leading to catastrophic deployment failures. By only testing the "happy path", the project is architecturally dishonest about its resilience. An adversary could easily bypass the agent's logic, manipulate its memory, or trigger unbounded recursion, compromising the host environment or data integrity.

**Recommended Structural Change:** Expand the `evals/` framework to include explicit adversarial test suites. This must include prompt injection tests, memory poisoning tests, unbounded recursion detection, and malformed tool output scenarios. Implement a red-team testing pipeline as a mandatory gate for any model or architecture changes.
