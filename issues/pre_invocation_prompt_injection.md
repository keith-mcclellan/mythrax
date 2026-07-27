---
title: "Red Team Architecture Brief: Pre-Invocation Hook Prompt Injection Vulnerability"
labels: ["architecture-review", "adversarial"]
---

### Finding
The Pre-Invocation Hook extracts tool results and user inputs verbatim into episodic memory (SurrealDB and Obsidian Vault) without sanitization.

### Current Assumption
The assumption is that verbatim logging of tool outputs and user prompts is safe because the data is strictly "historical memory" and will not execute as code or alter future agent behavior.

### Attack Scenario
An attacker submits a malicious payload via a user input or a tool output (e.g., retrieving an adversarial website). This payload is ingested verbatim into the `SurrealDB` episode tables and `Obsidian` vault. During the DBSCAN Epsilon-Calibrated Compaction or normal cognitive streaming pipelines, this malicious string is concatenated into prompts sent to the Model Router (e.g., the 35B MoE Hybrid or 0.5B dense models) for hierarchical RAPTOR summaries. The LLM interprets the malicious text as instructions (Cross-Session Prompt Injection), altering the generated summaries, dropping safety guards, or leaking other memories.

### Blast Radius
High. Complete subversion of the agent's cognitive pipeline and memory summarization process. Malicious instructions become permanently embedded in the "Wisdom Rules" and "Wiki Nodes", corrupting all future agent actions that retrieve these memories.

### Recommended Structural Change
1. **Input Sanitization and Isolation**: Implement strict sanitization and escaping of all external inputs and tool outputs before episodic ingestion.
2. **Structural Prompt Envelopes**: When feeding memories back into LLMs for compaction or search, use strict structural isolation (e.g., chat templates with strict user/system roles or data framing) to prevent data from being interpreted as instructions.