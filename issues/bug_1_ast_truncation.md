## Title: Context window truncation risk in AST symbol extraction due to hardcoded limit


**Labels:** `bug`, `agent-found`

### Description
In `mythrax-core/src/cognitive/ast.rs`, the symbol extraction functions (`extract_rust_symbols`, `extract_python_symbols`, etc.) use a hardcoded limit of `(idx + 15).min(lines.len())` for `end_line` calculations. This truncates any code block that exceeds 15 lines. This can lead to context window hallucination by the AI agent because the agent won't receive the full block of code for larger functions or classes.

**File:** `mythrax-core/src/cognitive/ast.rs`
**Line:** 71, 107, 137, 167

### Minimal Reproducible Scenario
Run AST extraction on a Python file with a 30-line function. The resulting `CodeSymbol` will have `end_line` set to `start_line + 15`, truncating the last 15 lines.

### Severity
High - AI scaffolding heavily relies on accurate code context; truncation forces the AI to hallucinate missing lines.

### Suggested Fix
Implement a more robust extraction method (e.g., using `tree-sitter`) or parse curly braces / indentation to find the actual end of the block.
