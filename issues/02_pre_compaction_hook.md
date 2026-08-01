---
title: "🛡️ Sentinel: [CRITICAL] Pre-compaction Hook Extracts Tool Results Verbatim without Sanitization"
labels: ["architecture-review", "adversarial", "bug", "agent-found"]
---

### Finding:
The pre-compaction hook in `mythrax-core/src/hooks/precompact.rs` extracts tool results and user inputs verbatim into episodic memory without sanitization.

### Current Assumption:
The assumption is that tool outputs and user inputs are safe to store and will not affect subsequent downstream processing or model completions.

### Attack Scenario:
An attacker crafts a malicious input or compromises a tool to return a prompt injection payload. When this verbatim payload is retrieved from memory during subsequent LLM invocations (e.g., via the streaming-to-disk cognitive pipeline or search retrieval), the LLM executes the injected instructions, leading to privilege escalation, data exfiltration, or further system compromise.

### Blast Radius:
CRITICAL. Allows persistent, cross-session prompt injection that poisons the long-term memory store and affects all future agent interactions that retrieve this memory.

### Recommended Structural Change:
- Implement mandatory input/output sanitization and escaping before storing episodic memory.
- Introduce a secure serialization format that strictly delineates control boundaries (e.g., system vs. user prompts) when memories are reconstructed for the LLM context.
