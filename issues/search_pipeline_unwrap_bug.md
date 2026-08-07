---
title: Panic in search_pipeline.rs on keyword_resp_res
labels: bug, agent-found
---

**File:** `mythrax-core/src/db/search_pipeline.rs`
**Line:** 1986

**Description:**
The code uses `.unwrap()` on `keyword_resp_res`, which is a `Result`. If the DB query returns an `Err`, this will panic and crash the daemon.

**Minimal Reproducible Scenario:**
Trigger a keyword search where the underlying SurrealDB query fails (e.g., due to a timeout or connection issue). The daemon will panic on line 1986 of `search_pipeline.rs` because `keyword_resp_res.unwrap()` will panic on the `Err` variant.

**Severity:** High (Crash)

**Suggested Fix:**
Propagate the error using the `?` operator or handle it appropriately instead of using `.unwrap()`.

```rust
let keyword_resp = keyword_resp_res?;
let mut keyword_candidates = parse_results(keyword_resp, false)?;
```
