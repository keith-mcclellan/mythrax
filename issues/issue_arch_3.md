---
title: "Red Team Architecture Brief: Pre-Invocation Hook Verbatim Ingestion Enables Prompt Injection"
labels: [architecture-review, adversarial]
---

### Finding
The `Pre-Invocation Hook` extracts text and tool output *verbatim* and ingests it into SurrealDB as episodic memories.

### Current Assumption
Downstream processing (compaction, AST symbol extraction, model execution) can safely handle untrusted, verbatim strings without execution risk.

### Attack Scenario
An attacker feeds malicious text (e.g., control tokens like `<|`, `|>` or code block syntax) into a tool output. The pre-invocation hook ingests it verbatim. When the Compactor Sweep Service synthesizes these verbatim records, the MoE Hybrid model misinterprets the control tokens, executing a prompt injection attack.

### Blast Radius
Model hijacking, allowing the attacker to manipulate cognitive artifacts, forge reference pages, or influence subsequent code generation/evaluations.

### Recommended Structural Change
Sanitize external inputs (user inputs, API tool results) in the `Pre-Invocation Hook` by neutralizing control tokens (`<|`, `|>`) and stripping executable syntax before storing them in prompt-visible episodic memory.
