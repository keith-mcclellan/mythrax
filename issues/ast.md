## Bug Report: Hardcoded context window truncation in ast.rs

**File:** `mythrax-core/src/cognitive/ast.rs`
**Line:** 69, 110, 149, 188

**Minimal Reproducible Scenario:**
In the symbol extraction functions, the end line of a symbol is hardcoded to a maximum of 15 lines via `let end_line = (idx + 15).min(lines.len());`. If a function or class exceeds 15 lines, the extracted code block is abruptly truncated. This limits the context window for LLMs and leads to hallucinations or incorrect understanding of the codebase structure since they lack the complete function logic.

**Severity:** Medium (Correctness/Context limitations)

**Suggested Fix:**
Implement a proper parser or bracket-matching heuristic to determine the true end of the symbol, rather than arbitrarily limiting it to 15 lines. Alternatively, use existing robust libraries (e.g., `tree-sitter`) for AST parsing.
