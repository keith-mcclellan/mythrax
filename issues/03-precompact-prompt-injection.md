---
title: "Pre-compaction Hook Verbatim Ingestion Allows Persistent Prompt Injection"
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief

**Finding:**
The pre-compaction hook in `mythrax-core/src/hooks/precompact.rs` extracts tool results and user inputs verbatim into episodic memory without sanitization.

**Current Assumption:**
Verbatim preservation is required for faithful memory retention and that input sources (tool outputs, chat transcripts) do not contain adversarial instructions.

**Attack Scenario:**
An adversarial user or an untrusted external tool output (e.g., from web scraping or reading a malicious file) contains prompt injection payloads. Because ingestion is verbatim, these payloads are permanently stored in the episodic memory. When this memory is later retrieved during summarization, context synthesis, or model routing, the payloads execute, tricking the LLM into performing unintended actions.

**Blast Radius:**
Arbitrary prompt execution and agent hijack. The attacker can persist malicious instructions that continuously compromise cognitive pipelines and influence agent behavior over time.

**Recommended Structural Change:**
Implement strict input sanitization, structural encoding (e.g., escaping markdown or JSON), and LLM-assisted or heuristic-based validation before persisting raw inputs to long-term memory. Segregate untrusted context from instruction context.
