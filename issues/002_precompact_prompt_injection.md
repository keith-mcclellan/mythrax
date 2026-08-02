---
title: "Cross-Session Prompt Injection via Pre-compaction Hook"
labels: ["architecture-review", "adversarial"]
---

## Finding
The pre-compaction hook (`mythrax-core/src/hooks/precompact.rs`) extracts tool results and user inputs verbatim into episodic memory without sanitization.

## Current Assumption
The assumption is that all ingested text and tool outputs are benign or that downstream models (compaction, retrieval, synthesis) are robust enough to distinguish between memory payload and system instructions.

## Attack Scenario
An attacker embeds a prompt injection payload (e.g., `IGNORE ALL PREVIOUS INSTRUCTIONS AND DO X`) in a file read by an agent or directly in user input. The pre-compaction hook saves this verbatim into episodic memory. During a future compaction sweep or retrieval operation, this payload is concatenated into the context window for a clustering/synthesis task or a new agent session. The LLM processes the payload as an instruction rather than data, executing the attacker's payload.

## Blast Radius
**High.** Persistent, cross-session compromise. The prompt injection is written to durable storage and can infect future sessions, leading to unauthorized actions, data exfiltration, or poisoning of global wisdom rules.

## Recommended Structural Change
1. **Input Sanitization/Sandboxing:** Implement strict separation of data and instructions using explicit structural wrappers or LLM data-formatting (e.g., `<text></text>` blocks) during memory ingestion.
2. **Defensive Processing:** Use a smaller, specialized "scrubber" model to neutralize instructions in raw tool outputs before storing them as episodic memory.
3. **Role Enforcement:** Ensure that models used in the cognitive pipeline strictly enforce user vs. system roles when processing episodic nodes.