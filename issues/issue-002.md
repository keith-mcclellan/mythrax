---
title: "Architecture Review: Unbounded Recursive Loop Risk via Verbatim Ingestion"
labels: ["architecture-review", "adversarial"]
---

**Finding:** Unbounded recursive loop in agent orchestration and memory compaction.

**Current Assumption:**
As documented in `ARCHITECTURE.md`: "The hook parses the session's JSONL transcripts line-by-line... Extracts the raw text and tool results verbatim, indexing them into SurrealDB as episodic memories without dropping any tool output details." The system assumes that raw tool outputs and agent transcripts are finite, benign, and safe to ingest verbatim into the database.

**Attack Scenario:**
Prompt injection through verbatim memory insertion. An adversarial input or compromised tool result forces the agent into a loop of generating excessively long or recursively self-referential outputs. Because the pre-compaction hook extracts and indexes these "verbatim... without dropping any tool output details," this massive influx of noisy episodic memory is ingested into SurrealDB. During the subsequent "dreaming" cycle, the system attempts to cluster and summarize these via DBSCAN and hierarchical RAPTOR trees.

**Blast Radius:**
The uncontrolled verbatim ingestion followed by heavy clustering logic can cause unbounded hierarchical RAPTOR tree generation. This leads to infinite loops or severe computational stalls during DBSCAN clustering, ultimately causing VRAM exhaustion for embedding models and disk I/O bottlenecks.

**Recommended Structural Change:**
Implement a strict bounded recursion depth and a hard token/length limit per session or per turn during verbatim ingestion. Introduce circuit breakers in the pre-compaction hook to truncate or drop excessively verbose tool outputs before they reach SurrealDB.
