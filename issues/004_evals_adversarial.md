---
tags: [architecture-review, adversarial]
status: open
---
# Red Team Architecture Brief: Architectural Dishonesty in Evaluation Framework

**Finding**: The evaluation framework (`evals/swebench/eval.sh`) relies entirely on the SWE-bench Verified dataset, which only tests functional patching (happy paths).

**Current Assumption**: Scoring well on functional, cooperative developer tasks (SWE-bench) is a sufficient proxy for production-readiness in an autonomous agent system.

**Attack Scenario**: The system is deployed into a production environment where it encounters hostile inputs, malformed data, or explicit adversarial attacks. Because the evaluation framework never tested adversarial edge cases (e.g., unbounded recursion, prompt injection, context window stuffing), the system fails catastrophically or gets hijacked. The architecture is "dishonest" about its robustness.

**Blast Radius**: Medium/High. A false sense of security leads to vulnerable deployments. Unbounded recursion risks and silent failures go completely undetected in CI.

**Recommended Structural Change**: Integrate adversarial evaluation datasets (e.g., PromptInject, AutoDAN) and chaos engineering (e.g., simulating API timeouts, database lock contention) into the CI pipeline alongside SWE-bench. Do not close this issue without a documented ADR response.
