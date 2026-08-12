---
title: Off-by-one or panic risk in BM25 retrieval
labels: bug, agent-found
---
# Bug Report

**Severity:** MEDIUM
**File:** `mythrax-core/src/retrieval/bm25.rs`
**Line:** 118, 119

## Minimal Reproducible Scenario
While not an explicit off-by-one, the code `let tf = *term_freqs.get(term).unwrap_or(&0) as f32;` followed by `let df_t = *self.df.get(term).unwrap_or(&0);` indicates an assumption. If `self.df` has an off-by-one or missing entry for a term that exists in `term_freqs`, `df_t` becomes 0. A subsequent division by `df_t` or use of `df_t` in log calculations (like IDF) could result in infinity or NaN, which propagates and breaks search rankings.

## Suggested Fix
Ensure that `df_t` is checked against 0 before being used as a divisor, or ensure the document frequency map is always correctly populated.
