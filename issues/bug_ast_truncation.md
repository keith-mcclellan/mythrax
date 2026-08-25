---
labels: bug, agent-found
---

**File:** `mythrax-core/src/cognitive/ast.rs`
**Line Numbers:** 69, 110, 149, 188

**Description:**
The symbol extraction functions hardcode the `end_line` calculation as `(idx + 15).min(lines.len())`. This truncates any code blocks (like functions, structs) longer than 15 lines, risking context window hallucination for the AI agent as it receives incomplete code definitions.

**Minimal Reproducible Scenario:**
1. Ingest a `.rs` file containing a function longer than 15 lines.
2. Observe the extracted `CodeSymbol` for the function. Its content is abruptly truncated after 15 lines.

**Severity:** High (leads to AI hallucination due to truncated context)

**Suggested Fix:**
Instead of a hardcoded 15-line limit, implement brace-matching or indentation-based parsing to accurately determine the actual end of the block.
