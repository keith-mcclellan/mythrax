---
title: BM25 IDF calculation risks negative values or NaN
labels: bug, agent-found
---
**File and Line:** `mythrax-core/src/retrieval/bm25.rs`, line 122

**Minimal Reproducible Scenario:** In a distributed or global stats scenario (e.g., using `with_global_stats`), if the provided global document frequency `df_t` exceeds the total document count `self.n` due to stale statistics or edge cases, the formula `(((self.n as f32 - df_t as f32 + 0.5) / (df_t as f32 + 0.5)) + 1.0).ln()` can attempt to take the natural log of a zero or negative number, silently returning `NaN` or a negative IDF score and breaking search ranking.

**Severity:** Medium

**Suggested Fix:** Clamp `df_t` to not exceed `self.n` and enforce a minimum positive floor for the IDF value (e.g., `.max(0.01)`) to ensure the term inside `ln()` is always valid and strictly positive.
