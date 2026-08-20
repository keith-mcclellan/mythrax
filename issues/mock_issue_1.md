# Bug: Potential Panic in Search Pipeline Keyword Extraction

**File:** `mythrax-core/src/db/search_pipeline.rs`
**Line:** 1986

## Minimal Reproducible Scenario
In `search_pipeline.rs`, if `keyword_resp_res` is `None` or an `Err`, calling `.unwrap()` on it at line 1986 will panic:
`let mut keyword_candidates = parse_results(keyword_resp_res.unwrap(), false)?;`
This occurs if the keyword query future fails or if `keyword_resp_res` is initialized as `None` (which can happen under certain conditions when `is_hybrid` is true, but `keyword_resp_res` ends up not being set properly).

## Severity
High - A panic in the core search pipeline crashes the node serving the query.

## Suggested Fix
Handle `keyword_resp_res` gracefully, defaulting to an empty list or returning an error if necessary, instead of unwrapping. For example:
`let mut keyword_candidates = if let Some(res) = keyword_resp_res { parse_results(res, false)? } else { Vec::new() };`