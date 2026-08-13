---
labels: bug, agent-found
---
## Medium Severity Issue in `mythrax-core/src/db/search_pipeline.rs`

**Line Number:** 1986
**Scenario:** If `keyword_resp_res` is `None` (no keyword response provided) or an `Err`, `.unwrap()` panics.
**Suggested Fix:** Use `match keyword_resp_res { Some(res) => parse_results(res, false)?, _ => Vec::new(), }`
