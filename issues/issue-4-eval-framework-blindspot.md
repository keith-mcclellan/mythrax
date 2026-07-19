---
tags: [architecture-review, adversarial]
---
# Architectural Dishonesty: Eval Framework Blindspot

## Finding
The evaluation framework relies entirely on the SWE-bench Verified dataset for performance scoring. It does not test adversarial inputs.

## Code Reference
`evals/swebench/eval.sh`

## Current Assumption
High performance on SWE-bench translates to a reliable and robust system in production.

## Attack Scenario
SWE-bench only evaluates code resolution and "happy path" coding capabilities. An attacker feeds adversarial inputs, prompt injections, or malformed data to the orchestration layer. The system fails catastrophically because it has never been evaluated against adversarial resilience.

## Blast Radius
Unquantified vulnerability to prompt injection, uncontrolled recursion, and logic breaking in production, despite passing all evaluations.

## Recommended Structural Change
Integrate a dedicated adversarial evaluation suite alongside SWE-bench. Test specifically for prompt injection resilience, agent boundary enforcement, and graceful degradation under malformed inputs.

**Note: Do not close this issue without a documented architectural decision record (ADR) response.**
