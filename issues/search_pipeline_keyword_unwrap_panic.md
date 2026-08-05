---
labels: ['bug', 'agent-found']
severity: High
---

# Panic on `.unwrap()` when keyword search DB query fails

**File:** `mythrax-core/src/db/search_pipeline.rs`
**Line Number:** 1986

## Description
There is a panic path when extracting keyword search candidates. The result `keyword_resp_res` is directly unwrapped without checking if it contains an error.

## Minimal Reproducible Scenario
If the SurrealDB database query fails during keyword search (e.g., due to connection drop, timeout, or malformed query string), `keyword_resp_res` will evaluate to `Err`. Attempting to `.unwrap()` this `Err` will crash the active process/daemon instead of returning a proper error to the API caller.

## Suggested Fix
Use the `?` operator or pattern matching to propagate the error up the stack instead of blindly calling `.unwrap()`.
