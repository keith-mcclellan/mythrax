---
tags: [architecture-review, adversarial]
---
# Finding: SWE-bench Happy-Path Evals

**Current Assumption:** `evals/swebench/` provides a comprehensive measure of system correctness and reliability.

**Attack Scenario:** The eval framework fails to test adversarial input robustness, context window overflow, or shell injection boundaries. Attackers can leverage untested edge cases (e.g., shell injection via raw POSIX shell invocations) that the SWE-bench framework completely ignores.

**Blast Radius:** Silent deployment of highly vulnerable orchestration logic, resulting in RCE or data exfiltration.

**Recommended Structural Change:** Introduce dedicated adversarial test harnesses in `evals/adversarial/` focusing on prompt injection, unbounded recursion, and input fuzzing.
