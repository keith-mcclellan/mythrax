---
title: Test Coverage Gap in extract_symbols
labels: bug, agent-found
---

**Bug Type:** Test coverage gaps
**File:** `mythrax-core/src/cognitive/paging.rs`
**Line:** 72
**Severity:** High
**Minimal Reproducible Scenario (Trigger):**
Critical agent virtual paging functions like `extract_symbols` and `page_code_block` in `mythrax-core/src/cognitive/paging.rs` lack test coverage. This introduces risks of silent extraction and code substitution failures that will break the context window management in the agent scaffolding. If a developer refactors the AST parser, there's no test to catch regressions in what's extracted.

**Suggested Fix:**
Add a `mod tests` block at the bottom of `paging.rs` with `#![cfg(test)]`. Add test cases that pass small snippets of code and verify that `extract_symbols` correctly returns the expected `Vec<Symbol>`, catching regressions for common languages like Rust, Python, and TypeScript.
