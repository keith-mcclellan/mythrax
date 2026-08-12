---
title: Critical Agent Virtual Paging Functions Lack Test Coverage
labels: bug, agent-found
---
# Bug Report

**Severity:** MEDIUM
**File:** `mythrax-core/src/cognitive/paging.rs`
**Line:** N/A (Coverage Gap)

## Minimal Reproducible Scenario
As noted in memory, critical agent virtual paging functions in `mythrax-core/src/cognitive/paging.rs` (such as `extract_symbols` and `page_code_block`) lack test coverage in the eval harness. These functions are crucial for context window management. If a logic bug is introduced, it could lead to silent extraction failures or code substitution errors that corrupt agent memory or result in hallucinatory code rewrites, bypassing normal constraints.

## Suggested Fix
Add comprehensive unit tests and integration tests in the eval suite specifically targeting `extract_symbols`, `page_code_block`, and related virtual paging mechanisms to ensure correct extraction boundaries, handling of edge cases (like multi-line strings, nested brackets, and truncated blocks), and proper symbol replacement under various constraints.
