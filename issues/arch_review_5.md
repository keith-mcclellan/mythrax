# Eval framework bias towards happy paths lacking adversarial coverage

**Labels**: `architecture-review`, `adversarial`

## Finding
The evaluation framework (`evals/swebench/`) tests verified, functional 'happy paths' (e.g., SWE-bench coding tasks) but lacks adversarial input robusteness or security boundary testing.

## Current Assumption
If the agent can solve known, benign coding problems, the system architecture and boundaries are sound.

## Attack Scenario
The system achieves high evaluation scores but fails catastrophically when deployed against malformed inputs, edge cases (e.g., massive AST symbol structures), or malicious code repositories that trigger path traversal or regex DOS.

## Blast Radius
False confidence in system reliability. When scaled 10x, unexpected data shapes and adversarial environments cause persistent panics, infinite loops, and security breaches.

## Recommended Structural Change
Introduce a dedicated adversarial evaluation suite (`evals/adversarial/`) that generates malformed JSON, recursive symlinks, infinitely long strings, and prompt injection attempts to validate boundary enforcement and graceful degradation.
