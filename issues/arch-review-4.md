# Adversarial Architecture Review: Eval framework tests only happy paths

Labels: architecture-review, adversarial

**Finding**: The evaluation framework in `evals/` lacks tests for adversarial inputs, prompt injection, or bounded execution limits.

**Current Assumption**: The evaluation framework adequately tests the cognitive capabilities of the agent across all scenarios.

**Attack Scenario**: An attacker provides a carefully crafted adversarial prompt (e.g., via a compromised dependency or maliciously named file) that bypasses the agent's constraints. Because the `evals/` framework only validates expected happy paths, the system will appear robust but fail silently or behave maliciously under attack.

**Blast Radius**: Unbounded recursion, prompt injection vulnerabilities, and failure to enforce agent scope boundaries.

**Recommended Structural Change**: Implement a Red Team evaluation suite within `evals/` that specifically injects adversarial payloads, tests memory consumption under stress (e.g., recursive tool calling), and validates that the agent fails safely and deterministically.