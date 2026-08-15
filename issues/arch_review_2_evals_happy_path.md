---
labels: architecture-review, adversarial
---

# Red Team Architecture Brief: Evals Framework (SWE-bench) is Architecturally Dishonest

**Note: This issue MUST NOT be closed without a documented Architectural Decision Record (ADR) response.**

**Finding** -> The evaluation framework in `evals/swebench/` wraps the official SWE-bench Verified harness but only tests standard model patching capabilities. It lacks explicit adversarial robustness testing for the agent orchestration itself.

**Current Assumption** -> Testing the LLM's ability to patch code (SWE-bench) is sufficient to validate the 'intelligence' and safety of the system.

**Attack Scenario** -> An attacker introduces malicious payloads in a codebase (e.g., prompt injection in comments or log files). When Mythrax ingests these files (AST symbol extraction or workspace doc sync), the agent processes the poisoned context, leading to unbounded recursion or extraction of sensitive vault data.

**Blast Radius** -> Compromise of agent behavior, leading to potential data exfiltration or system resource exhaustion via unbounded loops.

**Recommended Structural Change** -> Introduce a dedicated adversarial evaluation suite in `evals/` that explicitly tests prompt injection resistance, boundary enforcement, and recursion limits (e.g., feeding the system poisoned wikis or code containing malicious instructions).
