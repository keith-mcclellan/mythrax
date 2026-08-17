---
title: Hardcoded `end_line` in AST Extraction Truncates Code Symbols
labels: bug, agent-found
---

**Severity**: Medium
**File**: `mythrax-core/src/cognitive/ast.rs`
**Lines**: 69, 110, 149, 188

### Description
In `mythrax-core/src/cognitive/ast.rs`, the symbol extraction functions hardcode the `end_line` calculation as `(idx + 15).min(lines.len())`. This arbitrary 15-line limit truncates longer functions, classes, and structs, leading to incorrect evaluations and context window hallucinations.

### Minimal Reproducible Scenario
Run AST extraction on any `fn` or `class` longer than 15 lines. The resulting `CodeSymbol` will have its `end_line` set to `start_line + 15`, ignoring the rest of the body.

### Suggested Fix
Use a brace-counting or indentation-aware logic to accurately compute the `end_line` for each parsed block.
