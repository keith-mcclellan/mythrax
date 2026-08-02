---
title: "Evaluation Framework Fails to Test Adversarial Inputs"
labels: ["architecture-review", "adversarial"]
---

## Finding
The evaluation framework located in `evals/swebench/` solely focuses on verified, functional "happy paths" (SWE-bench coding benchmarks). It completely lacks tests for adversarial inputs or security boundary robustness.

## Current Assumption
The assumption is that if the LLM agents can successfully complete SWE-bench coding tasks without breaking, the system is reliable enough for production. It assumes that standard prompts and context window limits naturally restrict agents from malicious behaviors.

## Attack Scenario
Because adversarial inputs are not evaluated, a new release inadvertently weakens the prompt sandbox or parsing boundaries. An attacker supplies malicious code or prompt injections (e.g., via a compromised repository or manipulated issue ticket) that forces the agent to execute unauthorized commands or exfiltrate sensitive data. Since the eval framework only tests "happy paths," this vulnerability is not caught before deployment.

## Blast Radius
**High.** Silent regression of security boundaries. Deployments are falsely marked as safe while leaving the daemon exposed to prompt injection, data exfiltration, or unauthorized file system operations by autonomous agents.

## Recommended Structural Change
1. **Adversarial Red-Teaming in CI:** Introduce a dedicated suite of adversarial evals (e.g., prompt injection payloads, malformed JSON structures, excessive context lengths, unauthorized file access attempts).
2. **Boundary Enforcement Verification:** Assert that agents *fail* gracefully or reject operations outside their allowed scope when presented with malicious instructions.
3. **Fuzzing Evals:** Implement automated fuzzing of the LLM inputs and MCP tool arguments to identify edge-case crashes or unexpected behaviors.