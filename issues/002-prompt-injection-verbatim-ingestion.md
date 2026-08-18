---
labels: architecture-review, adversarial
state: open
---
# Red Team Architecture Brief: Prompt Injection Vulnerabilities in Verbatim Ingestion

**Finding** -> The Pre-Invocation Hook and Pre-Compaction processes ingest raw text, tool outputs, and AST metadata verbatim.
**Current Assumption** -> Downstream LLM interactions are resilient to malformed or malicious text payloads embedded in source code or task descriptions.
**Attack Scenario** -> A user analyzes a third-party repository containing a prompt injection in a comment or `README.md`. Mythrax ingests it verbatim. During retrieval or compaction, the LLM processes the injected text and executes unauthorized MCP tool commands.
**Blast Radius** -> Complete local environment compromise, including unauthorized file modification, data exfiltration via network calls, or poisoning of the `wisdom` rules to manipulate future agents.
**Recommended Structural Change** -> Introduce a prompt sanitation layer (LLM firewall) and strict RBAC for MCP tool execution. Differentiate between trusted system prompts and untrusted ingested data in API calls. Must require an ADR response to close.
