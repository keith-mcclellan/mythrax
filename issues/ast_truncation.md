**Title:** AST truncation logic truncates context
**Labels:** bug, agent-found
**File:** mythrax-core/src/cognitive/ast.rs
**Line:** 69, 110, 149, 188
**Severity:** Medium
**Minimal Reproducible Scenario:** Parsing a code block longer than 15 lines truncates the context because the end line is hardcoded to `(idx + 15).min(lines.len())`.
**Suggested Fix:** Parse until the closing brace or block end instead of hardcoding `idx + 15`.
