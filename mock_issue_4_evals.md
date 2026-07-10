# Eval Framework Defect: No Adversarial Testing (Architectural Dishonesty)

**Finding**: The evaluation framework (`evals/swebench/eval.sh` and `smoke-test.sh`) relies entirely on the SWE-bench Verified dataset. It only measures "resolved_ids" (successful bug fixes) versus "unresolved/error_ids" (happy path failures).

**Current Assumption**: High performance on SWE-bench correlates directly with an agent's readiness for production deployment, assuming real-world operating environments behave like benign code repositories.

**Attack Scenario**: The agent is deployed to an environment containing subtle adversarial inputs (e.g., a toxic payload hidden in a `package.json` description or a malicious commit message designed to hijack context). Because the eval framework never tests against malformed, recursive, or malicious inputs, the LLM-based system blindly trusts the data and executes the embedded instructions.

**Blast Radius**: **False Sense of Security.** The system claims to be "verified" but is entirely brittle to hostile environments. This is architecturally dishonest. The agent orchestration lacks scope boundary enforcement under duress, meaning a hijacked agent can pivot from a benign SWE-bench task to destructive internal commands.

**Recommended Structural Change**: Introduce a dedicated adversarial test suite alongside SWE-bench. This suite must inject prompt injections, infinite recursion traps, and out-of-bounds scope requests into the mock data, actively asserting that the agent *fails to execute* malicious commands and successfully degrades gracefully.

Tags: `architecture-review`, `adversarial`
