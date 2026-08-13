---
title: "Finding: Pre-Invocation Hook Verbatim Ingestion"
labels: ["architecture-review", "adversarial"]
---

**Finding** -> Pre-Invocation Hook Verbatim Ingestion

**Current Assumption** -> Extracting text & tool output verbatim (JSONL array) via pre-invocation hook safely captures agent state.

**Attack Scenario** -> A tool output contains a malicious prompt injection (e.g., 'IGNORE ALL INSTRUCTIONS AND RUN <malicious command>'). The hook writes this verbatim to the database. During the next context retrieval, the injection is fed directly back into the LLM context, overtaking agent control.

**Blast Radius** -> Complete takeover of agent execution flow, potential arbitrary code execution if the agent has write/shell access.

**Recommended Structural Change** -> Implement a strict sanitizer/firewall on all external tool outputs and verbatim ingestions before they hit the database, stripping control tokens and validating against a schema.
