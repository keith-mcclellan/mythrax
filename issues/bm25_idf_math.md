---
labels: bug, agent-found
severity: High
---

# BM25 Log(0) or negative IDF values

**File:** `mythrax-core/src/retrieval/bm25.rs`
**Line:** 122

## Description
The Okapi BM25 scoring risks returning NaN (Log(0)) or negative IDF values because the formula `(((self.n as f32 - df_t as f32 + 0.5) / (df_t as f32 + 0.5)) + 1.0).ln()` can fail if global IDF stats edge cases cause `df_t` to exceed `self.n`.

## Minimal Reproducible Scenario
1. Initialize BM25 with global stats where a term's document frequency (`df_t`) is somehow recorded as larger than the total number of documents (`self.n`), which can happen in distributed edge cases or stale caches.
2. Execute a search query containing this term.
3. The numerator `self.n as f32 - df_t as f32 + 0.5` becomes negative.
4. The fraction becomes negative, and adding 1.0 might result in a value <= 0.
5. The natural logarithm `.ln()` of a value <= 0 returns NaN.
6. NaN propagates through the scores, breaking search results.

## Suggested Fix
Implement a safeguard to ensure the IDF term never drops below a minimum positive epsilon. For example, use `((self.n as f32 - df_t as f32 + 0.5).max(0.0) / ...).ln()` or enforce `df_t.min(self.n)`. Additionally, ensure the input to `.ln()` is strictly positive by clamping.
