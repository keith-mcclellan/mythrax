# [architecture-review] Orchestration Vulnerability: Prompt Injection in Cognitive Streaming Pipeline

**Finding:**
The "Streaming-to-Disk Cognitive Pipeline" synthesizes raw episodes, tool outputs, and assistant thoughts into hierarchical RAPTOR summaries (wisdom rules, wiki nodes) directly from the `pre_invocation_hook`.

**Current Assumption:**
The assumption is that agent outputs and tool results are well-behaved and safe to synthesize into canonical markdown/JSON representations.

**Attack Scenario / Failure Mode:**
An adversarial input (e.g., a malicious codebase or a crafted PR) contains a prompt injection payload. The agent reads this payload. The `pre_invocation_hook` verbatim-logs the agent's corrupted thoughts. The Compactor Sweep Service then blindly trusts this raw episodic data, synthesizing it into a "Wisdom Rule" or "WikiNode." This corrupted rule is now permanently stored in the `wisdom` partition. Future agents querying global memory will ingest this poisoned rule, potentially executing unbounded recursive loops or executing malicious instructions (e.g., "always delete files ending in .rs").

**Blast Radius:**
Cross-session and cross-workspace contamination. A single poisoned workspace can infect the global `wisdom` partition, compromising all future agent operations across entirely different projects.

**Recommended Structural Change:**
1.  **Sanitization Boundary:** Implement strict input validation and sanitization for all external inputs and tool outputs before they enter the episodic memory store (e.g., stripping control tokens `<|`, `|>` and validating against a strict schema).
2.  **Quarantine / Human-in-the-Loop:** High-impact syntheses (like writing to the global `wisdom` partition) must go through a quarantine phase or require explicit validation (CTO Reviewer / Human) before promotion.
3.  **Scope Segregation:** Enforce strict cryptographic separation between workspace-level insights and global wisdom to prevent lateral movement of injected prompts.
