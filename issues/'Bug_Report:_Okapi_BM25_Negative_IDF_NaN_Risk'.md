## Bug Report: Okapi BM25 Negative IDF NaN Risk

**File & Line:** `mythrax-core/src/retrieval/bm25.rs`, line 122 (`let idf = (((self.n as f32 - df_t as f32 + 0.5) / (df_t as f32 + 0.5)) + 1.0).ln();`)

**Minimal Reproducible Scenario:** Edge cases in global statistics (e.g., if `df_t` slightly exceeds `self.n`) can result in negative IDF scores or zero arguments to `.ln()`, leading to `NaN` scores, breaking agent retrieval.

**Severity:** Medium (Retrieval Degradation).

**Suggested Fix:** Apply `.max(0.0)` after calculation: `let idf = ((((self.n as f32 - df_t as f32 + 0.5) / (df_t as f32 + 0.5)) + 1.0).ln()).max(0.0);`
