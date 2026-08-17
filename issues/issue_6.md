---
title: Missing test coverage for `cognitive::paging` functions
labels: bug, agent-found
---

**File:** `mythrax-core/src/cognitive/paging.rs` (or `cognitive::ast.rs`)
**Line:** N/A

**Severity:** Medium (Correctness)

**Issue:**
Critical agent virtual paging functions and AST extraction functions (such as `extract_code_ast` and `extract_rust_symbols`) lack test coverage, introducing risks of silent extraction failures and code substitution failures that can break the agent's ability to navigate large files.

**Minimal Reproducible Scenario:**
Look at `mythrax-core/src/cognitive/ast.rs` or `paging.rs`. There are no `#[cfg(test)]` modules or test functions verifying that the regex-based symbol extractors work correctly on varying inputs.

**Suggested Fix:**
Add a comprehensive test suite in the relevant module to verify `extract_code_ast` handles Rust, Python, Go, and TypeScript signatures correctly, especially testing edge cases like async functions, generics, and multi-line signatures.
