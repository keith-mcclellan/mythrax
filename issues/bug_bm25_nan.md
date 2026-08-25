---
labels: bug, agent-found
---

**File:** `mythrax-core/src/retrieval/bm25.rs`
**Line Numbers:** 122

**Description:**
The IDF formula is `(((self.n as f32 - df_t as f32 + 0.5) / (df_t as f32 + 0.5)) + 1.0).ln()`. If `df_t` (document frequency of term) exceeds `self.n` (total corpus size), which is possible when overriding global stats in `with_global_stats`, the term `(self.n as f32 - df_t as f32 + 0.5)` can become negative. This leads to taking the logarithm of a negative number, resulting in `NaN` scores. Even if not `NaN`, it can result in negative IDF values.

**Minimal Reproducible Scenario:**
1. Create a corpus with `n=10`.
2. Set `df_t=20` (e.g. from an edge case in global stats override).
3. The formula evaluates to `((10 - 20 + 0.5)/(20 + 0.5) + 1.0).ln() = (-9.5/20.5 + 1.0).ln() = (0.536).ln() = -0.62`.
(If `df_t` was larger it could hit `ln(negative)` -> `NaN`).

**Severity:** Medium (breaks search ranking retrieval)

**Suggested Fix:**
Ensure `df_t <= self.n` or use a safer IDF formulation like `((self.n as f32 - df_t as f32 + 0.5).max(0.0) / (df_t as f32 + 0.5) + 1.0).ln()` or clamp the IDF to a non-negative minimum.
