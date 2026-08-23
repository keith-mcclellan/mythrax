# Bug: Negative IDF causing NaN in BM25 scoring

**File and Line Number**: `mythrax-core/src/retrieval/bm25.rs`, line 122

**Minimal Reproducible Scenario**:
In edge cases (such as eventually consistent distributed indices or test data), if the term document frequency `df_t` exceeds the total document count `self.n`, the formula `(self.n as f32 - df_t as f32 + 0.5) / (df_t as f32 + 0.5)` can become `<-1.0`. Adding `1.0` yields a negative number. Calling `.ln()` on a negative number returns `NaN`, silently corrupting downstream agent retrieval scores.

**Severity**: Medium

**Suggested Fix**:
Clamp the inner term to a minimum positive epsilon before taking the logarithm, e.g., `(((...).max(0.0001)) + 1.0).ln()`, or clamp `df_t` to be `<= self.n`.

**Labels**: `bug`, `agent-found`