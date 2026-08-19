# Prompt Injection via Verbatim Episodic Memory

**Finding** -> The system preserves episodic memories verbatim, storing them via the SurrealDB database and formatting them with `vault::watcher::format_episode_markdown`.
**Current Assumption** -> Retrieval context is inherently trustworthy and does not contain malicious instructions.
**Attack Scenario** -> An adversary embeds a prompt injection in a downloaded file or external webpage. Mythrax ingests it verbatim into an episode. When retrieved via hybrid search, the payload commandeers the LLM.
**Blast Radius** -> Agent takeover, potential unauthorized code execution via agent tools.
**Recommended Structural Change** -> Introduce an LLM-based or deterministic sanitation layer prior to retrieving and injecting memories into the prompt buffer.


**Labels**: `architecture-review`, `adversarial`

*Note: Do not close this issue without a documented architectural decision record (ADR) response.*
