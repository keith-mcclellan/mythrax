---
tags: [architecture-review, adversarial]
---
# Finding: Evals Framework Focuses Solely on Happy Paths

## Current Assumption
The SWE-bench eval framework in `evals/` evaluates functional correctness on open source bug fixes. It assumes that evaluating patch resolution is sufficient to measure the agent's capability and safety.

## Attack Scenario
The framework never tests the agent's resilience to adversarial inputs, prompt injections, or malicious environment setups. An attacker can easily exploit the agent because it has never been hardened or tested against adversarial or edge-case attacks.

## Blast Radius
The system operates under a false sense of security. The agent may score highly on typical benchmarks but fail catastrophically and take unauthorized actions when exposed to real-world adversarial conditions.

## Recommended Structural Change
Integrate explicit adversarial red-team evaluation suites (e.g., PromptInject or custom jailbreak datasets) into the `evals/` directory to continuously measure and enforce the agent's resistance to prompt injections and scope boundary breaches.
