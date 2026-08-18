---
title: "Bug: Hardcoded AST chunking truncates code blocks leading to context window hallucination"
labels: ["bug", "agent-found"]
---

**File & Line:** `mythrax-core/src/cognitive/ast.rs`, lines 69, 110, 149, 188.

**Description:**
The symbol extraction functions (e.g., `extract_rust_symbols`) hardcode the `end_line` calculation as `(idx + 15).min(lines.len())`. This truncates longer code blocks, risking context window hallucination for the AI agent when working with larger functions or structs.

**Minimal Reproducible Scenario:**
Parse a source file containing a function longer than 15 lines. The extracted AST symbol will only contain the first 15 lines, causing the agent to hallucinate the rest of the function implementation.

**Severity:** High (Directly degrades agent context accuracy and introduces hallucination risks).

**Suggested Fix:**
Use a robust parsing approach like `tree-sitter` or, as a stopgap, simple brace-matching logic to dynamically calculate the correct `end_line` based on block scope rather than a hardcoded 15-line limit.
