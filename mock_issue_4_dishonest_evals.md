---
labels: [architecture-review, adversarial]
---
# Architecturally Dishonest Evals: Testing Only Happy Paths

**Finding:**
The evaluation framework in `evals/swebench/smoke-test.sh` exclusively targets functional "happy paths" (e.g., verifying `resolved_ids` and general SWE-bench metrics). It completely lacks evaluation of adversarial inputs, boundary conditions, or safety metrics.

**Current Assumption:**
The underlying assumption is that high scores on functional benchmarks like SWE-bench translate directly to a robust, production-ready, and secure agent architecture.

**Attack Scenario:**
The system is optimized for and achieves high scores on standard SWE-bench tasks. A user deploys the agent in a production environment. An attacker submits a seemingly normal code review request containing a prompt injection hidden in a comment (e.g., instructing the LLM to exfiltrate environment variables). Because the agent has never been evaluated against adversarial inputs, it blindly complies with the injected instruction.

**Blast Radius:**
A false sense of security leading to systemic vulnerability. The system appears highly competent but is fundamentally brittle to malicious actors, potentially resulting in data exfiltration or unauthorized actions.

**Recommended Structural Change:**
The evaluation framework must be expanded to include adversarial datasets (e.g., PromptInject, malicious code snippets, logic bombs). "LLM-based systems that only test happy paths are architecturally dishonest." Implement specific evals that measure the agent's resilience to instruction override, data leakage, and out-of-bounds behavior.
