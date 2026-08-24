**Title:** BM25 scoring can return NaN or negative IDF
**Labels:** bug, agent-found
**File:** mythrax-core/src/retrieval/bm25.rs
**Line:** 122
**Severity:** Medium
**Minimal Reproducible Scenario:** In okapi BM25 scoring, if global IDF stats edge cases cause `df_t` to exceed `self.n`, the log expression `(((self.n as f32 - df_t as f32 + 0.5) / (df_t as f32 + 0.5)) + 1.0).ln()` applies to a zero or negative number, yielding NaN.
**Suggested Fix:** Clamp the value before `.ln()` or use a safe IDF formula like `max(0, ...)`.
