---
title: "Eval Framework Lacks Adversarial Testing"
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief

**Finding:**
The evaluation framework in `evals/swebench/` solely focuses on verified, functional 'happy paths' (SWE-bench coding benchmarks) and fails to test adversarial input robustness or security boundaries.

**Current Assumption:**
LLM systems that pass functional coding benchmarks (like SWE-bench) are resilient and production-ready.

**Attack Scenario:**
Attackers deploy adversarial inputs (prompt injections, obfuscated malicious code, or out-of-distribution commands) against the agent. Because the system's defenses were only tuned for cooperative, well-behaved tasks, the agents bypass security controls or trigger edge-case architectural failures, such as unbounded token generation or logic flaws in tool execution.

**Blast Radius:**
Unpredictable systemic failures under adversarial load. The system is structurally dishonest about its reliability, leading to a false sense of security before deployment in hostile environments.

**Recommended Structural Change:**
Introduce adversarial red-teaming evals, fuzzing frameworks, and negative testing into the `evals/` suite. Ensure CI/CD pipelines evaluate agents against known injection vectors and bounded resource constraints, not just happy-path coding tasks.
