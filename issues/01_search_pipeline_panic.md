---
title: Panic in hybrid search when keyword_resp_res is None
labels: bug, agent-found
---

**File:** `mythrax-core/src/db/search_pipeline.rs`
**Line:** 1986

**Bug Type:** Panic Path (`unwrap()` on `Option`)

**Description:**
In hybrid search mode, the code assumes `keyword_resp_res` is `Some` and blindly calls `.unwrap()`. If the keyword search portion fails or was skipped (e.g. returning `None` instead of a response), this will panic the entire search pipeline, which is a critical operation.

**Minimal Reproducible Scenario:**
Run a hybrid search where the underlying FTS/keyword query returns an error or is optionally disabled/skipped in the query builder, causing `keyword_resp_res` to be `None`.

**Severity:** High (Crash in search pipeline)

**Suggested Fix:**
Use `if let` or `unwrap_or_default()` / handle the `None` case gracefully.
```rust
let mut keyword_candidates = if let Some(k_resp) = keyword_resp_res {
    parse_results(k_resp, false)?
} else {
    Vec::new()
};
```
