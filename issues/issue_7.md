---
labels: bug, agent-found
---
## High Severity Issue in `mythrax-core/src/cognitive/paging.rs`

**Line Number:** 72
**Scenario:** The functions `extract_symbols` and `page_code_block` are public but have no test coverage, leading to silent failures in virtual paging.
**Suggested Fix:** Add unit tests for `extract_symbols` and `page_code_block` covering multiple languages (Rust, TS, JS, Py).
