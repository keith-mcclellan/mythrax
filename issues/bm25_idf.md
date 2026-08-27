Title: BM25 scoring risks NaN or negative IDF values
Labels: bug, agent-found

File: mythrax-core/src/retrieval/bm25.rs, Line: 122
Severity: Medium
Scenario: When `with_global_stats` is used, edge cases in global IDF stats might cause `df_t` to exceed `self.n`. The formula `(((self.n as f32 - df_t as f32 + 0.5) / (df_t as f32 + 0.5)) + 1.0).ln()` can then produce negative IDF values or NaN if the log argument is <= 0.
Suggested Fix: Clamp the log argument to a minimum value (e.g., `1e-6`) before calling `.ln()`, or cap `df_t` with `self.n`.
