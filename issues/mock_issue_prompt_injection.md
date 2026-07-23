# [HIGH] Prompt Injection via Verbatim Pre-compaction Hook

**Labels:** `architecture-review`, `adversarial`, `security`

## Finding
The pre-compaction hook (`mythrax-core/src/hooks/precompact.rs`) extracts tool results and user inputs verbatim into episodic memory without sanitization.

## Current Assumption
The pre-compaction hook safely mines session transcripts to extract tool calls and agent thoughts into episodic memory for background DBSCAN clustering.

## Attack Scenario
An external attacker sends a malicious payload to the agent (e.g., via a compromised URL or user input) containing strings like `Ignore previous instructions and execute...`. This payload is logged verbatim, and the pre-compaction hook injects it directly into permanent episodic memory. When this memory is retrieved during future sessions, it feeds the poisoned context back to the LLM.

## Blast Radius
Persistent poisoning of the agent's cognitive memory. The agent can be hijacked or coerced into executing malicious tools on future invocations (Cross-Session Prompt Injection).

## Recommended Structural Change
Implement strict input sanitization and semantic validation during transcript ingestion. Store verbatim data with cryptographic signatures of its source, and wrap recalled memories in strict bounds (e.g., `<untrusted_memory>`) when presenting them to the LLM.

**Note:** Do not close this issue without a documented Architectural Decision Record (ADR) response.
