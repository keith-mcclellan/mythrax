---
title: Eval framework vulnerability: missing adversarial robustness testing
labels: bug, agent-found
---

**File:** `evals/swebench/eval.sh`
**Line:** 1

**Minimal Reproducible Scenario:**
The `swebench` evaluation harness solely tests functionality against verified SWE-bench functional "happy paths" (coding benchmarks) and uses `run_evaluation` via Docker. However, it lacks any mechanism to test for adversarial input robustness, context window poisoning, unbounded recursion, or security boundaries. Since agents operate on external unverified inputs in the codebase, the lack of an adversarial testing harness means the models can be easily manipulated or crashed by malicious inputs in production.

**Severity:** High (Agents lack defense validation against adversarial inputs, leading to prompt injection and potential system compromise).

**Suggested Fix:**
Incorporate an adversarial evaluation suite alongside the functional SWE-bench testing. This suite should test edge cases like prompt injections (`<|`, `|>`, etc.), deeply nested recursive structures, excessively large context windows, and malformed inputs to validate security boundary logic and panic handling.
