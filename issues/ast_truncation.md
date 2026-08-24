---
title: Hardcoded AST symbol truncation causes context window hallucination
labels: bug, agent-found
---
**File and Line:** `mythrax-core/src/cognitive/ast.rs`, lines 69, 110, 149, 188

**Minimal Reproducible Scenario:** When processing a function or struct that is longer than 15 lines, the extracted AST block is forcibly truncated at `idx + 15`. If an AI agent attempts to read this symbol for context, it receives truncated code without indication, causing it to potentially hallucinate the missing logic.

**Severity:** High

**Suggested Fix:** Replace the hardcoded `(idx + 15).min(lines.len())` calculation with proper bracket/indentation counting or by utilizing a language-specific tree-sitter parser to extract the exact bounds of the symbol.
