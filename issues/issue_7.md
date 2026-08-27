---
title: "NaN Generation in BM25 Scoring"
labels: ["architecture-review", "adversarial"]
---

Finding -> Current Assumption -> Attack Scenario -> Blast Radius -> Recommended Structural Change

**Finding**: retrieval/bm25.rs Okapi BM25 scoring can return NaN (Log(0)) or negative IDF values.
**Current Assumption**: Global document count will always exceed document frequency.
**Attack Scenario**: Edge case ingestion causes df_t to equal or exceed self.n.
**Blast Radius**: Search scores become NaN, destroying the hybrid search index and blinding the agent.
**Recommended Structural Change**: Clamp minimum IDF values to a positive epsilon.

**Note:** Never close this issue without a documented architectural decision record (ADR) response.
