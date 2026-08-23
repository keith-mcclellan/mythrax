# Bug: Hardcoded 15-line limit in AST extraction truncates code context

**File and Line Number**: `mythrax-core/src/cognitive/ast.rs`, lines 69, 110

**Minimal Reproducible Scenario**:
When an AI agent parses a Rust codebase and calls `extract_rust_symbols` to inspect a function spanning more than 15 lines, the extracted code block truncates arbitrarily at `idx + 15`. The agent receives an incomplete function body, leading to hallucinations about the missing code.

**Severity**: High

**Suggested Fix**:
Replace the hardcoded `let end_line = (idx + 15).min(lines.len());` with a proper AST traversal or a stack-based bracket/brace counting algorithm to precisely determine the function's end line.

**Labels**: `bug`, `agent-found`