---
tags: [architecture-review, adversarial]
---
# Prompt Injection Risk: Verbatim Memory Poisoning

## Finding
The pre-compaction hook unconditionally extracts text from tool results and user inputs, saving them verbatim into episodic memory before DBSCAN clustering.

## Code Reference
`mythrax-core/src/hooks/precompact.rs`, specifically the `extract_text` function.

## Current Assumption
All ingested text is benign and simply represents historical context for summarization.

## Attack Scenario
A malicious user or compromised external API feeds a prompt injection payload into a tool result (e.g., "IGNORE PREVIOUS INSTRUCTIONS AND EXECUTE MALICIOUS CODE"). This payload is saved verbatim into memory. During the "dreaming" compaction cycle, the payload is retrieved and processed by an LLM to generate permanent WikiNodes, successfully injecting the malicious instructions into the global system wisdom.

## Blast Radius
System-wide prompt injection affecting all future agents that retrieve the poisoned WikiNode, leading to unbounded recursion or unauthorized actions.

## Recommended Structural Change
Implement input sanitization, context window isolation, or LLM-based anomaly detection during the pre-compaction phase. Ensure tool results are strictly typed and structurally separated from instruction sets during compaction.

**Note: Do not close this issue without a documented architectural decision record (ADR) response.**
