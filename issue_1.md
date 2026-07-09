---
labels: bug, agent-found
---

# Off-by-one error / Indexing logic flaw in `mcp_routes.rs` slice bounds calculation

## Location
- File: `mythrax-core/src/mcp_routes.rs`
- Lines: 2355, 2491, 2593

## Description
In `slice_content_by_lines` and `resolve_placeholders` related replacement blocks, the line index logic for `start_line` uses:
`let start_idx = start.map(|s| s.saturating_sub(1)).unwrap_or(0);`
Since the API assumes 1-indexed lines (e.g., passing `start_line = 1` evaluates to `index 0`), passing `start_line = 0` calculates `0.saturating_sub(1) = 0`, resulting in index `0`. Thus, passing 0 or 1 maps to the same index. An explicit bound check or safer calculation `.max(1)` is missing before subtraction.

## Minimal Reproducible Scenario
1. Send a request to an editing MCP tool (e.g., `replace_with_git_merge_diff` or equivalent file slicing tool) with `start_line: 0`.
2. The logic resolves `start_idx` as 0, modifying the first line instead of throwing an error for an invalid 0-index or correctly interpreting the slice boundaries. This can cause off-by-one corruption in edits when LLMs mistakenly zero-index lines.

## Severity
Medium. This leads to subtle file corruption during autonomous agent edits.

## Suggested Fix
Ensure input parameters are strictly clamped to a minimum of 1 before applying the offset:
```rust
let start_idx = start.map(|s| s.max(1).saturating_sub(1)).unwrap_or(0);
```