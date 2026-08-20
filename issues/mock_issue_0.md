# Bug: Hardcoded Truncation in AST Symbol Extraction

**File:** `mythrax-core/src/cognitive/ast.rs`
**Line:** 69, 110, 149, 188

## Minimal Reproducible Scenario
When parsing a file with `extract_code_ast`, the system extracts symbols with lines limited to a fixed length:
`let end_line = (idx + 15).min(lines.len());`
If a function, struct, class, or other AST symbol spans more than 15 lines, the extracted code stored in the AI agent's memory will be truncated.

## Severity
Medium - Limits the agent's ability to reason about and modify large code blocks correctly.

## Suggested Fix
Improve parsing logic to find the actual end of the symbol, potentially by counting braces (`{`, `}`) or indentation levels, rather than hardcoding a 15-line limit.