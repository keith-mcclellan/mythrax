---
labels: bug, agent-found
severity: Medium
---

# Context window hallucination due to hardcoded AST line limit

**File:** `mythrax-core/src/cognitive/ast.rs`
**Line:** 69, 110, 149, 188

## Description
Symbol extraction functions use a hardcoded limit of `(idx + 15).min(lines.len())` for `end_line` calculations. This truncates code blocks exceeding 15 lines and risks context window hallucination by feeding incomplete logic to the LLM context.

## Minimal Reproducible Scenario
1. Define a function or struct in the codebase that is 30 lines long.
2. Run the AST extraction pipeline (`extract_code_ast`).
3. The extracted code symbol for this block will cut off at 15 lines.
4. When the agent uses this symbol for reasoning, it lacks the latter half of the block, leading to hallucinations about missing variables or logic.

## Suggested Fix
Instead of using a hardcoded +15 offset, implement a proper AST-aware parsing strategy to determine the actual syntactic end line of the block (e.g., by matching closing braces or using `tree-sitter` bounds). If a heuristic must be used, increase the limit significantly and use indentation-based block detection.
