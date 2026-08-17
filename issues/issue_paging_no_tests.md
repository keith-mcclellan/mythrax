---
title: Critical Agent Virtual Paging Functions Lack Test Coverage
labels: bug, agent-found
---

**Severity**: High
**File**: `mythrax-core/src/cognitive/paging.rs`

### Description
The functions `extract_symbols` and `page_code_block` in `mythrax-core/src/cognitive/paging.rs` are critical for agent context window management and code extraction but currently have **zero test coverage**. This lack of testing introduces severe risks of silent extraction and code substitution failures, particularly with edge cases in various programming languages.

### Minimal Reproducible Scenario
1. Run test coverage checks on `mythrax-core`.
2. Notice there are no tests exercising `cognitive/paging.rs`.

### Suggested Fix
Implement comprehensive unit tests in `mythrax-core/src/cognitive/paging.rs` (in a `tests` module) that verify behavior across Rust, Python, TS/JS, and edge cases.
