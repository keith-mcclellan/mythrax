---
labels: architecture-review, adversarial
---
# Adversarial Review: Eval framework in `evals/swebench/` tests only happy paths (Eval Robustness)

## Finding
The evaluation framework in `evals/swebench/` solely focuses on verified, functional 'happy paths' (SWE-bench coding benchmarks) and fails to test adversarial input robustness or security boundaries.

## Current Assumption
The current assumption is that if an LLM agent can pass functional coding benchmarks (SWE-bench), it is safe and robust enough for production.

## Attack Scenario
An attacker provides malicious inputs (e.g., prompt injections, directory traversal attempts, or logic bomb code) to the system. Because the eval framework never tests these scenarios, the system accepts the malicious input, leading to unauthorized code execution, data exfiltration, or system compromise.

## Blast Radius
Complete compromise of the agent's operating environment and any connected systems or data, as security boundaries were never properly evaluated.

## Recommended Structural Change
Introduce adversarial datasets into the `evals/` framework. Include tests for prompt injection, jailbreaking, resource exhaustion, and unauthorized access attempts. Fail the build if the system cannot securely handle these adversarial inputs.