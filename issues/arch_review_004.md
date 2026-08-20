# Red Team Architecture Brief

## Finding
Adversarial Inputs Not Tested in Evals

## Current Assumption
The evaluation framework (`evals/`) tests the 'happy paths' of the LLM and orchestration logic, assuming standard inputs will validate system correctness.

## Attack Scenario
The system relies on LLMs to enforce scope boundaries and validate cognitive artifacts. Adversaries can supply malformed ASTs, deeply nested recursive structures, or contradictory instructions that are never tested in `evals/`. When deployed, the LLM hallucinates or panics on these edge cases, breaking the `code_symbol` table index or generating corrupted Vault entries.

## Blast Radius
Silent data corruption in the AST index and Obsidian Vault, leading to degraded agent performance over time.

## Recommended Structural Change
Mandate adversarial testing in `evals/`. Introduce chaos engineering for inputs: prompt injections, malformed JSON, recursive ASTs, and context window overflow payloads. Fail the build if the system does not gracefully reject or safely process these inputs.
