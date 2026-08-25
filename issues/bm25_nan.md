---
title: BM25 Okapi scoring can return NaN or negative IDF
labels: bug, agent-found
---

**File:** `mythrax-core/src/retrieval/bm25.rs`
**Line Number:** 122

**Description:**
The Okapi BM25 scoring risks returning NaN (Log(0)) or negative IDF values because the formula `(((self.n as f32 - df_t as f32 + 0.5) / (df_t as f32 + 0.5)) + 1.0).ln()` can fail if global IDF stats edge cases cause `df_t` to exceed `self.n`.

**Minimal Reproducible Scenario:**
Create a scenario where document frequency (`df_t`) is greater than the total number of documents (`self.n`), which can happen in edge cases of asynchronous global IDF updates or inconsistent stats. The calculation inside the natural log `ln()` can become less than or equal to zero, producing NaN or a negative score.

**Severity:** High

**Suggested Fix:**
Implement a safety clamp or a different BM25 variant (like BM25L or BM25+) that bounds the IDF to be strictly positive. For example, `f32::max(0.0, formula)` or modifying the equation to avoid negative values before the `.ln()` operation.
