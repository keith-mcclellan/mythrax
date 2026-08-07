---
title: Test coverage gaps in mythrax-core cognitive module
labels: bug, agent-found
---

**File:** `mythrax-core/src/cognitive/paging.rs`
**Line:** 1

**Description:**
There are public functions in `mythrax-core/src/cognitive/paging.rs` (`extract_symbols`, `page_code_block`, `intercept_and_restore_symbols`) with no corresponding test in the test suite. Given they form the core of the virtual paging mechanism for agent scaffolding, bugs here can lead to false positives where the code silently succeeds but produces an incorrect or incomplete code substitution.

**Minimal Reproducible Scenario:**
Run the test suite `cargo test` and observe that there are no tests specifically targeting `paging.rs` symbol extraction logic, which could silently fail to match complex structures (e.g., deeply nested generic `impl` blocks in Rust).

**Severity:** Medium (Coverage)

**Suggested Fix:**
Implement a test module inside `paging.rs` with `#[cfg(test)]` covering `extract_symbols`, `page_code_block`, and `intercept_and_restore_symbols` with various code snippets (Rust, Python, TS) to ensure correct context management.
