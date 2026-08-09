---
title: Missing test coverage for virtual paging functions
labels: bug, agent-found
---

**File:** `mythrax-core/src/cognitive/paging.rs`
**Line:** 72 (`extract_symbols`), 103 (`page_code_block`)

**Description:**
Critical agent virtual paging functions such as `extract_symbols` and `page_code_block` completely lack unit test coverage. This creates a significant blind spot in the eval harness.

**Minimal Reproducible Scenario:**
Modify the symbol extraction regex or the `extract_symbols` logic. The changes will compile cleanly but could silently fail to extract code blocks correctly at runtime, substituting broken context into the agent's memory window without any tests catching the regression.

**Severity:** High

**Suggested Fix:**
Add comprehensive unit tests for `extract_symbols` and `page_code_block` in `mythrax-core/src/cognitive/paging.rs` or the test suite to verify correct behavior on various edge cases and malformed code blocks.