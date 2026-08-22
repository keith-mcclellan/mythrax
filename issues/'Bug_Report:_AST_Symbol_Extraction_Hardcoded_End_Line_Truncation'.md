## Bug Report: AST Symbol Extraction Hardcoded End Line Truncation

**File & Line:** `mythrax-core/src/cognitive/ast.rs`, lines 69, 110, 149, 188 (e.g., `let end_line = (idx + 15).min(lines.len());`)

**Minimal Reproducible Scenario:** Run symbol extraction on a >15 line Rust/Python/TS/Go function. Agent context windows truncate the rest of the function.

**Severity:** High (Agent hallucination risk).

**Suggested Fix:** Implement a proper bracket/brace matching parser to accurately determine the `end_line` of the symbol block.
