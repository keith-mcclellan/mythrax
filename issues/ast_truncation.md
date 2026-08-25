---
title: Context window truncation in ast.rs
labels: bug, agent-found
---

**File:** `mythrax-core/src/cognitive/ast.rs`
**Line Numbers:** 69, 110, 149, 188

**Description:**
The symbol extraction functions (e.g., `extract_rust_symbols`) hardcode the `end_line` calculation as `(idx + 15).min(lines.len())`. This truncates longer code blocks, risking context window hallucination for the AI agent when evaluating context that exceeds 15 lines.

**Minimal Reproducible Scenario:**
Feed a function that spans 20 lines into the symbol extraction pipeline. The extracted symbol context will only contain the first 15 lines, causing the AI agent to operate on incomplete code and potentially hallucinate the rest.

**Severity:** High

**Suggested Fix:**
Instead of hardcoding a 15-line limit, properly parse the AST to determine the actual start and end lines of the symbol (e.g., using `tree-sitter` or similar parser bounds) and extract the exact block.
