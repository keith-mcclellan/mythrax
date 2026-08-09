---
title: Panic Risk via unwrap in Hybrid Search Pipeline
labels: bug, agent-found
---

**Bug Type:** Panic Paths (unwrap/expect)
**File:** `mythrax-core/src/db/search_pipeline.rs`
**Line:** 1986
**Severity:** High
**Minimal Reproducible Scenario (Trigger):**
In `search_pipeline.rs` around line 1986, when parsing the keyword response in hybrid search mode, the code assumes `keyword_resp_res` is `Some`:
`let mut keyword_candidates = parse_results(keyword_resp_res.unwrap(), false)?;`
If `is_hybrid` is true, but `keyword_resp_res` is `None` (for example, if the keyword query failed or was bypassed due to some DB error), this will panic and crash the search retrieval query.

**Suggested Fix:**
Check if `keyword_resp_res` is `Some` before unwrap:
```rust
let mut keyword_candidates = if let Some(k_resp) = keyword_resp_res {
    parse_results(k_resp, false)?
} else {
    Vec::new()
};
```
