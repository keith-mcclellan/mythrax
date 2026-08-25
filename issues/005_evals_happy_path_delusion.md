---
tags: architecture-review, adversarial
---

# Evaluation Framework Happy Path Delusion

**Finding**: The eval framework in `evals/swebench/` tests exclusively against `princeton-nlp/SWE-bench_Verified`.

**Current Assumption**: Passing SWE-bench Verified proves the agent is robust, capable of end-to-end coding performance, and safe for production deployment.

**Attack Scenario**: The agent successfully completes standard tasks on the benchmark, but fails completely or executes malicious instructions when fed an issue report containing prompt injections, obfuscated malicious code, or socially engineered tasks designed to breach the system boundaries.

**Blast Radius**: High confidence in a fundamentally vulnerable system. LLM-based systems that only test happy paths are architecturally dishonest. Deployment in production leads to immediate compromise when facing real-world adversarial data.

**Recommended Structural Change**: Introduce an adversarial SWE-bench variant (e.g., issues containing embedded prompt injections, backdoored dependencies) to explicitly test the agent's resilience, refusal rates, and boundary enforcement capabilities.
