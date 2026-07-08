# Logic Bug: Silent panic vulnerability on missing keyword response in hybrid search

**Labels:** `bug`, `agent-found`
**Severity:** High

## Vulnerability Details
**File:** `mythrax-core/src/db/backend.rs`
**Line:** 3311

In the `search` function, when `is_hybrid` is true and `vector_resp_res` is `Some`, the code assumes `keyword_resp_res` is also `Some` and blindly calls `.unwrap()` on it: `let mut keyword_candidates = parse_results(keyword_resp_res.unwrap(), false)?;`. If a query is provided where vector search succeeds but keyword search fails or returns `None`, this will panic and crash the daemon.

**Code Snippet:**
```rust
let mut keyword_candidates = parse_results(keyword_resp_res.unwrap(), false)?;
```

## Minimal Reproducible Scenario
1. Start the `mythrax-core` daemon with hybrid search enabled in the configuration.
2. Ingest documents into the SurrealDB instance.
3. Send an API query that triggers a successful vector search but causes the keyword search backend to error or timeout (e.g., by dropping the full-text search index during query execution, or injecting an invalid FTS term syntax that is not caught earlier).
4. Observe the daemon crash via an unhandled panic at `mythrax-core/src/db/backend.rs:3311`.

## Suggested Fix
Safely unpack `keyword_resp_res` using `if let Some(resp) = keyword_resp_res` or fallback to an empty vector instead of panicking.
