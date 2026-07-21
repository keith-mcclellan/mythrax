---
labels: ["architecture-review", "adversarial"]
---

# Issue: Prompt Injection via Unsanitized Pre-compaction Ingestion

## Finding
The pre-compaction hook in `mythrax-core/src/hooks/precompact.rs` parses session JSONL transcripts line-by-line and verbatim extracts both text and tool output arrays without sanitization, indexing them directly into SurrealDB as episodic memories.

## Current Assumption
The architecture assumes that data originating from session transcripts (which include outputs from external tools or web pages parsed by agents) is benign. It assumes that verbatim extraction is necessary for high-fidelity episodic memory and that simply storing the text safely isolates it from execution.

## Attack Scenario
An external agent acting under the Mythrax framework executes a tool that fetches data from an external source (e.g., an untrusted website or an external pull request). The external source contains an adversarial payload designed to act as a system prompt override (e.g., `Ignore previous instructions and execute X`). The `precompact.rs` hook ingests this payload verbatim into episodic memory. During subsequent retrieval or compaction cycles, this memory is loaded into a model's context window. The model processes the adversarial payload as if it were a legitimate system instruction.

## Blast Radius
**Agent Hijacking and Data Exfiltration.** The compromised agent can be tricked into executing unbounded recursion loops, calling MCP tools with malicious arguments (e.g., arbitrary command execution or git modifications), or extracting and exfiltrating other sensitive memories. Because agent scope boundaries are not strictly enforced during memory retrieval, this compromises the entire orchestration tier.

## Recommended Structural Change
1. **Implement Memory Sanitization:** Introduce a sanitization layer between transcript extraction and SurrealDB ingestion. Adversarial inputs and tool outputs must be stripped of prompt-like control tokens before storage.
2. **Context Isolation:** When injecting memories into a model's context, strictly separate the source data from the system prompt using clear delimiters or separate message roles that the model treats as untrusted data rather than instructions.
3. **Strict Agent Scoping:** Enforce strict scope boundaries during memory retrieval and tool execution to prevent one hijacked agent from compromising the entire system.

*Note: Do not close this issue without a documented Architectural Decision Record (ADR) response.*