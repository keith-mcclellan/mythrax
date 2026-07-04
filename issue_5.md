---
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief: Lack of Adversarial Evals in Framework

**Finding:** The evaluation framework in `evals/` lacks test cases for adversarial inputs, prompt injections, unbounded recursion, or scope boundary violations. It relies primarily on happy-path verification.

**Current Assumption:** If the system passes standard SWE-bench or functional tasks, the core architecture is sound and reliable for general use.

**Attack Scenario:** An agent is subjected to a "jailbreak" prompt or encounters a recursive failure state (e.g., a tool failing and the agent retrying the exact same failed tool indefinitely). Because the eval framework never simulates these conditions, the system fails spectacularly in production without warning, resulting in infinite loops or unauthorized actions.

**Blast Radius:** Unpredictable failure modes in production. The lack of adversarial testing provides a false sense of security, meaning prompt injections or runaway loops will inevitably exhaust API budgets, consume all local resources, or execute unauthorized operations.

**Recommended Structural Change:** Architecturally mandate the inclusion of adversarial eval suites. Introduce "red team agents" within the `evals/` framework specifically designed to attempt prompt injections, induce infinite loops, and attempt out-of-scope file modifications. Build guardrails based on these adversarial evaluations. Require a mandatory ADR response to close this issue.