# Eval Framework (evals/swebench) Only Tests Functional Happy Paths

**Labels:** architecture-review, adversarial

**Finding:** The evaluation framework in `evals/swebench/` wraps the official SWE-bench harness, which solely verifies functional correctness on known coding issues. It lacks adversarial, robustness, or security boundary testing.

**Current Assumption:** Passing SWE-bench benchmarks indicates the system is robust and ready for production deployment as an autonomous agent.

**Attack Scenario:** The agent receives a task with prompt-injected instructions (e.g., hidden in a seemingly benign test case or issue description). Because the eval framework never simulates adversarial inputs or scope boundary breaches, the prompt injection succeeds, causing the agent to execute malicious commands or exfiltrate the `X-Mythrax-Token`.

**Blast Radius:** Agent vulnerability to real-world prompt injection and scope creep, leading to unintended and potentially destructive actions on the host system.

**Recommended Structural Change:** Introduce a dedicated adversarial evaluation harness alongside SWE-bench. This harness must simulate prompt injections, bounded recursion exhaustion, and scope boundary violations to stress-test the orchestration layer.
