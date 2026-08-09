---
title: Unsafe unwrap on database search results
labels: bug, agent-found
---

**File:** `mythrax-core/src/db/search_pipeline.rs`
**Line:** 1986 (`let mut keyword_candidates = parse_results(keyword_resp_res.unwrap(), false)?;`)

**Description:**
The code calls `.unwrap()` on `keyword_resp_res` when parsing database query results. This creates a panic risk (DoS vulnerability) if the database query fails or returns a malformed structure.

**Minimal Reproducible Scenario:**
Submit a search query containing invalid syntax, triggering a database engine error. The search pipeline encounters an `Err` variant in `keyword_resp_res`, which panics when `.unwrap()` is called, crashing the daemon or search process.

**Severity:** High

**Suggested Fix:**
Avoid using `.unwrap()` and propagate the error instead, using `?` or `match` block:
```rust
let keyword_candidates_resp = keyword_resp_res.map_err(|e| e.into())?;
let mut keyword_candidates = parse_results(keyword_candidates_resp, false)?;
```