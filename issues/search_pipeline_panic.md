---
labels: bug, agent-found
---

# Panic on failed database query in search_pipeline.rs

## Location
- **File**: `mythrax-core/src/db/search_pipeline.rs`
- **Line**: 1986

## Minimal Reproducible Scenario
When performing a search, the code unwrap()'s the `keyword_resp_res` query result. If the underlying database operation fails (e.g., due to connection loss, a syntax error in the generated query, or timeouts), `keyword_resp_res` will be an `Err`, causing the `unwrap()` to panic and immediately crash the server.

## Severity
High

## Suggested Fix
Propagate the error using the `?` operator instead of calling `unwrap()`, allowing the request to fail gracefully.
```rust
let keyword_resp = keyword_resp_res?; // Propagate the error if the query failed
let mut keyword_candidates = parse_results(keyword_resp, false)?;
```
