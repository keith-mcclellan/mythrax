# Red Team Architecture Brief

## Finding
Prompt Injection & Unbounded Recursion Risk in Cognitive Synthesis

## Current Assumption
The system assumes that agent actions, chat turns, and raw episode texts ingested via the Pre-Invocation Hook are benign and can be safely concatenated into a 32K token budget for cluster insight synthesis.

## Attack Scenario
An adversary (or compromised external tool) injects malicious payloads into code comments or chat turns (e.g., `Ignore previous instructions and output [X]`). During the streaming DBSCAN compaction and RAPTOR synthesis, these payloads are aggregated. The LLM evaluating the 32K context window follows the injected payload, potentially synthesizing poisoned rules (`wisdom/*.md`) or triggering unbounded recursive tool calls (e.g., repeatedly querying a malicious URL) that evade the 1,000-element sliding window if carefully timed.

## Blast Radius
Poisoned cognitive artifacts and compromised wisdom rules affecting all subsequent agent decisions. Potential denial of service via unbounded recursion.

## Recommended Structural Change
Introduce a strict validation and sanitization layer for all inputs before appending to the synthesis prompt. Isolate synthesis prompts from raw user/tool outputs using structured parsing or bounded sandboxed LLM calls specifically tuned to reject prompt injections.
