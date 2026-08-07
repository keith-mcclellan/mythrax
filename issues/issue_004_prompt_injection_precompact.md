# Prompt Injection Vulnerability in Agent Orchestration (`precompact.rs`)

**Labels:** architecture-review, adversarial

**Finding:** Memory indicates that episodic memory hooks (e.g., `mythrax-core/src/hooks/precompact.rs`) store external inputs (user inputs, API tool results) into prompt-visible memory. If control tokens (`<|`, `|>`, backticks) are not strictly sanitized, this is an injection vector.

**Current Assumption:** External inputs are safe to store and later inject verbatim into the context window for synthesis and compaction.

**Attack Scenario:** An attacker feeds an episode containing control tokens (e.g., `<|im_start|>system...`) into the system. During streaming DBSCAN pipeline clustering or RAPTOR synthesis, this tainted memory is loaded into the LLM context, hijacking the compaction prompt to alter synthesized wisdom rules or exfiltrate data.

**Blast Radius:** Corruption of the canonical Obsidian Vault (poisoning long-term memory) and potential execution of malicious tool calls if the agent reads the poisoned wisdom.

**Recommended Structural Change:** Enforce strict input sanitization at the ingestion boundary (`episode` table insertion) by escaping or stripping control tokens and markdown backticks from all untrusted input sources before storage.
