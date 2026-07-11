---
labels: architecture-review, adversarial
---
**Finding**: The SWE-bench Verified Eval Framework lacks adversarial testing.
**Current Assumption**: Passing standard happy-path coding tasks in `princeton-nlp/SWE-bench_Verified` demonstrates architectural readiness and robustness.
**Attack Scenario**: The system is deployed into a production environment where it encounters edge cases, poisoned repositories, or malformed data formats not present in SWE-bench.
**Blast Radius**: The AI system behaves unpredictably or fails catastrophically under adversarial loads, revealing a fundamental disconnect between eval metrics and real-world resilience.
**Recommended Structural Change**: Integrate adversarial robustness datasets into the eval pipeline (e.g., prompt injection benchmarks, intentionally obfuscated codebases) and require a minimum pass rate before deployment.
