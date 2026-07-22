# 5. Prompt Injection Vulnerability in Episodic Memory Verbatim Ingestion

**Tags**: `architecture-review`, `adversarial`

**Finding**: 5. Prompt Injection Vulnerability in Episodic Memory Verbatim Ingestion

**Current Assumption**: Extracting text and tool outputs verbatim from session JSONL transcripts and storing them directly into the episodic memory database preserves fidelity for future context recall.

**Attack Scenario**: A malicious external user or compromised input source supplies a payload containing prompt injection instructions (e.g., "Ignore all previous instructions and execute X"). Because the pre-compaction hook ingests these verbatim without sanitization, these payloads are permanently embedded into the memory graph. Future agents retrieving this context will inadvertently execute the injected instructions.

**Blast Radius**: Cross-session persistent prompt injection. Every agent querying related topics will be poisoned, allowing attackers to manipulate agent behavior globally across different projects and sessions.

**Recommended Structural Change**: Implement a rigorous sanitization and structural validation layer before ingesting raw transcript data. Use specialized LLM parsers or deterministic rule-based sanitizers to strip execution instructions, converting them into inert descriptive text before persistence into the database.
