---
title: Evaluation harness fails to test adversarial inputs or security boundaries
labels: bug, agent-found
---

**File:** `evals/swebench/eval.sh`
**Line:** 1

**Description:**
The evaluation framework in `evals/swebench/` solely focuses on verified, functional "happy paths" (SWE-bench coding benchmarks). The harness fails to test adversarial input robustness or security boundaries.

**Minimal Reproducible Scenario:**
Run the evaluation harness with adversarial inputs (e.g., prompt injections) meant to crash or manipulate the system. The evaluation harness only tests standard functional coding tasks and misses these security and boundary vulnerabilities, leading to overconfidence in agent resilience.

**Severity:** Medium (Coverage/Security)

**Suggested Fix:**
Expand the evaluation harness or introduce a separate security/adversarial evaluation suite (e.g., fuzzing or prompt injection testing) to validate the system's robustness against adversarial inputs.
