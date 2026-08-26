## Title: BM25 IDF calculation risks returning NaN or negative values


**Labels:** `bug`, `agent-found`

### Description
In `mythrax-core/src/retrieval/bm25.rs`, the Okapi BM25 scoring calculates IDF using the formula:
`(((self.n as f32 - df_t as f32 + 0.5) / (df_t as f32 + 0.5)) + 1.0).ln()`
If the global IDF stats have edge cases where `df_t` (document frequency of term) exceeds `self.n` (total number of documents), the term inside the logarithm can become zero or negative. `(0.0).ln()` results in negative infinity, and negative numbers result in NaN.

**File:** `mythrax-core/src/retrieval/bm25.rs`
**Line:** 121

### Minimal Reproducible Scenario
Load BM25 with `global_stats` where `global_n` is 10, but a specific term has a `global_df` of 11. Run a query with that term. The score will result in NaN due to negative input to `.ln()`.

### Severity
Medium - Corrupts search retrieval scores and may cause downstream panic if NaNs are unhandled in sorting.

### Suggested Fix
Ensure the input to `.ln()` is bounded to be greater than 0, or cap `df_t` to `self.n`. Alternatively, use a safe IDF formula like `(self.n as f32 / (df_t as f32 + 1.0)).ln() + 1.0`.
