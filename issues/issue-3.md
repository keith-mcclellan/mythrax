---
title: "Adversarial Architecture Review: Systemic Prompt Injection in Cognitive Pipeline"
labels: architecture-review, adversarial
---

**Finding:** The streaming-to-disk cognitive pipeline (DBSCAN clustering, RAPTOR summaries, AST extraction) ingests raw episode and vault content without intermediate adversarial sanitization.
**Current Assumption:** Memory content (episodes, wiki nodes, code ASTs) is benign and can be safely concatenated and summarized by the LLM up to a 32K token limit or truncated at 100K characters.
**Attack Scenario:** A user or external system drops a file containing cross-session prompt injection payloads (e.g., `<|im_start|>system...`) into the workspace vault. The `sync_workspace_docs_to_vault` or episodic ingestion pipeline reads it verbatim. When the Compactor Sweep Service synthesizes RAPTOR summaries, the LLM executes the injected instructions, permanently persisting malicious cognitive artifacts into the core knowledge base.
**Blast Radius:** Unbounded propagation of adversarial payloads across the memory graph, leading to agent hijacking, unbounded recursion (if the payload commands the agent to generate more files), and persistent scope boundary violations.
**Recommended Structural Change:** Introduce a mandatory sanitization layer for all external inputs (masking control tokens, encoding backticks) before storing them in episodic memory or feeding them to the summarization pipeline. Implement an adversarial classifier in the pre-invocation hook.

Never close without a documented architectural decision record (ADR) response.
