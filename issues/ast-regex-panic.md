---
labels: bug, agent-found
---
# Bug: Regex Panic and Off-by-one in AST Extractor
**File:** mythrax-core/src/cognitive/ast.rs:45,69,110,149,188
**Scenario:** `Regex::new(...).unwrap()` can panic if the regex pattern is malformed, and `let end_line = (idx + 15).min(lines.len());` hardcodes the block length, truncating longer code blocks and causing context hallucination for the agent.
**Severity:** High (Panic path & Data loss)
**Fix:** Replace hardcoded lengths with proper brace/scope tracking logic, and use lazy statics like `OnceLock::new()` for regex initialization with proper error propagation rather than `.unwrap()`.