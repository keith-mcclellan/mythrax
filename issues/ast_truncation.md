Title: AST extraction hardcodes 15-line limit, risking context window hallucination
Labels: bug, agent-found

File: mythrax-core/src/cognitive/ast.rs, Lines: 69, 110, 149, 188
Severity: Medium
Scenario: When parsing Rust, Python, TS, or Go files for code symbols, `end_line` is hardcoded to `(idx + 15).min(lines.len())`. Code blocks longer than 15 lines are truncated, which risks context window hallucination.
Suggested Fix: Implement proper bracket or block parsing instead of a hardcoded 15-line limit, or expand the context retrieval logic.
