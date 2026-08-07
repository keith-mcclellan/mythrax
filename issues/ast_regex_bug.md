---
title: Incomplete regex for Rust generic functions
labels: bug, agent-found
---

**File:** `mythrax-core/src/cognitive/ast.rs`
**Line:** 45

**Description:**
The regex `^\s*(?:pub(?:\([^)]+\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(fn|struct|enum|trait|type|const)\s+([A-Za-z0-9_]+)` is used to parse Rust functions. It doesn't correctly account for generic type parameters (e.g., `fn name<T>(...)`), silently missing complex function signatures if they are formatted without a space before the `<`.

**Minimal Reproducible Scenario:**
Given a Rust function `pub fn parse_data<T>(input: T) -> Result<()> {}`, the regex will match `parse_data` as the identifier, but it will truncate the `<T>` part. This causes missing information in the AST extraction, rendering cognitive/AST analysis incomplete for generic types.

**Severity:** Medium (Correctness)

**Suggested Fix:**
Update the regex to account for generic parameters:
```rust
Regex::new(r"^\s*(?:pub(?:\([^)]+\))?\s+)?(?:async\s+)?(?:unsafe\s+)?(fn|struct|enum|trait|type|const)\s+([A-Za-z0-9_]+(?:<[^>]+>)?)").unwrap()
```
