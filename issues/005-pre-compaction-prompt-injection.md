---
title: "🛡️ Sentinel: [CRITICAL] Pre-Compaction Verbatim Prompt Injection"
labels: ["architecture-review", "adversarial", "bug", "agent-found"]
---

# Vulnerability Report: Cross-Session Prompt Injection via Pre-Compaction Hook

## Finding
The pre-compaction hook (`hooks/precompact.rs` -> `mythrax-core/src/api.rs`) extracts tool results and user inputs verbatim from JSONL transcripts into episodic memory without any sanitization or structural separation.

## Current Assumption
Past transcripts are inherently safe to re-ingest as plain text and will not maliciously influence future cognitive processes when retrieved by the LLM.

## Attack Scenario
An attacker injects a malicious prompt payload into a tool result (e.g., via a compromised webpage read by an agent). This payload is stored verbatim in the database. During future memory retrieval or daily compaction (dreaming), the LLM reads this verbatim memory and executes the injected prompt, altering its behavior or exfiltrating data.

## Blast Radius
**Cross-session prompt injection.** This allows passive, persistent adversarial control over the agent's cognitive loops, leading to unauthorized actions or data exfiltration long after the initial malicious interaction.

## Recommended Structural Change
Implement strict input sanitization and structure boundaries for episodic memories. Use a robust meta-prompting or quoting mechanism when recalling memories so the LLM is explicitly instructed to treat them strictly as data, preventing the execution of embedded instructions.

---
*Note: Do not close this issue without a documented Architectural Decision Record (ADR) response.*