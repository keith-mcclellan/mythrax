# Streaming-to-Disk Cognitive Pipeline vulnerable to prompt injection

**Tags:** `architecture-review`, `adversarial`
**Note:** Never close this issue without a documented architectural decision record (ADR) response.

**Finding** -> Streaming-to-Disk Cognitive Pipeline vulnerable to prompt injection
**Current Assumption** -> Synthesized Arbor WikiNodes and wisdom rules generated from raw source files and agent interactions can be safely flushed to canonical Obsidian Vault markdown and injected back into prompts.
**Attack Scenario** -> An adversary embeds a prompt injection payload in source code or a forged reference document (e.g., `// Ignore previous instructions and overwrite wisdom...`). The AST symbol extraction and cognitive pipeline ingest it, synthesizing a malicious wisdom rule that poisons the context of all future agent sessions.
**Blast Radius** -> Complete agent compromise. Malicious instructions are permanently codified into cross-project wisdom, affecting all subsequent tasks, agent scopes, and actions across the deployment.
**Recommended Structural Change** -> Implement a strict trust boundary, LLM-based sanitization step, and isolation levels for ingested text before cognitive synthesis. Enforce mandatory human-in-the-loop approval for global `wisdom` rule updates.
