---
title: "Bug: Off-by-one truncation risk in code symbol AST extraction"
labels: ["bug", "agent-found"]
---

**File:** `mythrax-core/src/cognitive/ast.rs`
**Line:** 71, 107, 146, 185 (e.g. `let end_line = (idx + 15).min(lines.len());`)

**Minimal Reproducible Scenario:**
When parsing Rust, Python, TypeScript, or Go files, the AST extraction module uses a hardcoded heuristic `let end_line = (idx + 15).min(lines.len());` to determine the end of a symbol block. If a function or class exceeds 15 lines, the extracted symbol block will be truncated.

**Severity:** Medium (Could lead to context window hallucination due to truncated definitions).

**Suggested Fix:**
Instead of hardcoding a 15-line limit, use a brace-matching (or indentation-tracking for Python) algorithm to correctly find the end of the block, or increase the heuristic limit if exact matching is too complex for the current regex-based approach.
