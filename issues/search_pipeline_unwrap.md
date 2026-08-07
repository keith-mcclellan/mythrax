---
title: Panic vulnerability via .unwrap() on keyword_resp_res in search_pipeline.rs
labels: bug, agent-found
---

**File:** `mythrax-core/src/db/search_pipeline.rs`
**Line:** 1986

**Minimal Reproducible Scenario:**
If the database query for the keyword search fails (returning an `Err`), `keyword_resp_res` will be an `Err` variant. When the code calls `keyword_resp_res.unwrap()`, it will panic, crashing the entire daemon. This could be triggered by realistic runtime inputs such as a malformed search query, temporary database unavailability, or an unexpected data format in the index.

**Severity:** Critical (can cause a Denial of Service by crashing the application on certain inputs or DB states).

**Suggested Fix:**
Handle the `Result` gracefully by propagating the error instead of calling `.unwrap()`.

```rust
let mut keyword_candidates = parse_results(keyword_resp_res?, false)?;
// Or if you want to fall back to an empty vector:
// let mut keyword_candidates = if let Ok(k_resp) = keyword_resp_res {
//     parse_results(k_resp, false)?
// } else {
//     Vec::new()
// };
```
