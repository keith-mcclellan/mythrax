# Non-Adversarial Happy-Path Eval Framework

**Tags:** `architecture-review`, `adversarial`

**Finding:** The `evals/swebench/eval.sh` framework tests only standard functional datasets (SWE-bench) and lacks adversarial testing.
**Current Assumption:** Functional "happy path" testing is sufficient to validate agent capabilities and architectural robustness.
**Attack Scenario:** An attacker injects hidden or malicious text into a dataset. Since the system has never been tested against adversarial prompt manipulation or context window stuffing, the agent blindly trusts and executes the payload.
**Blast Radius:** The architecture is fundamentally dishonest about its resilience, exposing the deployed system to trivial prompt-injection and data exfiltration.
**Recommended Structural Change:** Integrate adversarial evaluation suites (e.g., PromptInject, Garak) directly into the `evals/` pipeline. Fail builds if agents comply with out-of-scope instructions.
