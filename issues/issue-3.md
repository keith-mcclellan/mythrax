---
title: "Bug: Test Coverage Gap in mythrax-core Public Functions"
labels: ["bug", "agent-found"]
---

**File & Line:** `mythrax-core/src/cognitive/ast.rs`, line 20

**Description:**
There are multiple public functions within `mythrax-core` that have no corresponding test in the eval or test suite. This lack of test coverage makes it difficult to verify their behavior and increases the risk of regressions.

**Minimal Reproducible Scenario:**
Examine the `mythrax-core/src/cognitive/ast.rs` file. Functions like `extract_code_ast` are marked as `pub`, but they do not have direct tests confirming they operate correctly.

**Severity:** Medium (Lack of test coverage for core functionality).

**Suggested Fix:**
Add comprehensive unit and integration tests for all `pub` functions in `mythrax-core` to ensure edge cases and core functionalities are verified.
