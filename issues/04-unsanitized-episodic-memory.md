---
title: "Pre-compaction Hook Ingests Unsanitized Raw Episodic Memory"
labels: [architecture-review, adversarial]
---

**Finding:** Pre-compaction Hook Ingests Unsanitized Raw Episodic Memory.

**Current Assumption:** Extracting tool results and user inputs verbatim into episodic memory is necessary for accurate historical context and debugging.

**Attack Scenario:** Cross-session prompt injection. An attacker inputs a malicious payload which is persisted verbatim. When the system later summarizes or retrieves this memory, the model executes the injected instructions.

**Blast Radius:** Delayed model hijacking, data corruption, or unauthorized tool execution, persisting across sessions.

**Recommended Structural Change:** Sanitize inputs before ingestion. Use structural tags or an LLM-based sanitation proxy to neutralize actionable payloads.
