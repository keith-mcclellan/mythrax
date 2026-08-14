---
title: Token budget truncation only drops a single line
labels: bug, agent-found
---

**File:** `mythrax-core/src/mcp_routes/manage_handlers.rs`
**Lines:** 2281, 2293

**Bug Type:** Off-by-one / Logic Error

**Description:**
When truncating lines for the token budget, the code does:
```rust
let truncated_lines = &lines[..lines.len() - 1];
```
This always removes exactly one line from the end, regardless of how many lines actually need to be dropped to fit the token budget. If it's vastly over budget, dropping just 1 line doesn't fix it. If it was already under budget, why truncate? (It seems it's inside a loop or conditional, but dropping exactly 1 line is arbitrary and likely an off-by-one or logic error in token management). Additionally, if `lines.len()` is 0 (though checked for `> 1`), this kind of manual index manipulation is risky.

**Minimal Reproducible Scenario:**
Provide an STM working memory payload that is significantly larger than the token budget (e.g., thousands of lines). The current logic will only truncate a single line, causing the prompt to remain over budget and potentially failing the downstream LLM call due to context window limits.

**Severity:** Medium (LLM Context Window Overflow)

**Suggested Fix:**
Calculate the actual number of tokens/lines to remove based on the budget, rather than a hardcoded `- 1`.
