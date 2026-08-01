---
title: "🛡️ Sentinel: [MEDIUM] Eval Framework Only Tests Happy Paths"
labels: ["architecture-review", "adversarial"]
---

### Finding:
The evaluation framework in `evals/swebench/` solely focuses on verified, functional 'happy paths' (SWE-bench coding benchmarks) and fails to test adversarial input robustness or security boundaries.

### Current Assumption:
The assumption is that correctness on standard coding tasks implies overall system robustness and safety.

### Attack Scenario:
Because adversarial inputs (like prompt injection, malformed data, or large payloads) are never tested during evaluation, regressions or vulnerabilities in input handling, memory safety, or LLM parsing go unnoticed until exploited in production.

### Blast Radius:
MEDIUM. Weakens the overall security posture and allows vulnerabilities to slip into releases undetected. Architecturally dishonest as it presents a false sense of security.

### Recommended Structural Change:
- Introduce adversarial red-teaming test suites in the `evals/` framework.
- Include fuzzing and prompt-injection payloads in standard evaluation runs.
