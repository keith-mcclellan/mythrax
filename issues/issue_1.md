            ---
            title: Bug: panic in search_pipeline.rs from unwrapping keyword_resp_res
            labels: bug, agent-found
            ---

            **File:** `mythrax-core/src/db/search_pipeline.rs`
**Line:** 1986

**Scenario:**
If `keyword_resp_res` is `None`, calling `unwrap()` causes a panic during hybrid search when `fts_cap` truncation is attempted. It seems this occurs when `mode` is neither 'keyword' nor 'vector' and `query_emb` is populated but `fts_words` somehow leads to `keyword_resp_res` being unassigned, or similar edge cases in hybrid paths where `keyword_resp_res` might be None. Actually, `keyword_resp_res` is set to `None` when `query_emb` is Some and it falls to `(Some(vector_fut.await), None)` block on line 1349. Then later at line 1986 it unwraps `keyword_resp_res.unwrap()`, leading to panic.

**Severity:** High (Runtime panic processing search queries).
**Suggested Fix:** Use `if let Some(k_resp) = keyword_resp_res { parse_results(k_resp, false)? } else { Vec::new() }` to safely handle the None case.
