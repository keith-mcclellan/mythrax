# Prompt Injection Risks in "Verbatim" Pre-Compaction Hooks

**Tags:** `architecture-review`, `adversarial`

**Finding:** The pre-compaction hook ingests verbatim string or block transcripts (including raw tool results) into episodic memory without sanitization.
**Current Assumption:** Ingesting transcripts verbatim preserves context safely for future compaction and synthesis.
**Attack Scenario:** An external payload (e.g., untrusted web content) containing a prompt injection disguise as a tool output is ingested. During the daily DBSCAN clustering and Arbor HTR loop, the LLM executes the injection, altering permanent rules.
**Blast Radius:** Complete pollution of permanent `wiki_node` and wisdom rule storage, creating an unbounded recursion loop of poisoned context during daily dreaming cycles.
**Recommended Structural Change:** Introduce a strict sanitization and boundary-enforcement layer between raw episodic ingestion and the compactor. Wrap retrieved verbatim memories in strict system boundaries before LLM synthesis.
