---
title: Off-by-one in clean_json_payload strips valid content
labels: bug, agent-found
---

**File:** `mythrax-core/src/cognitive/prompts.rs`
**Line:** 323

**Bug Type:** Off-by-one Error

**Description:**
In `clean_json_payload`, when stripping markdown code blocks, the code does:
```rust
lines[1..lines.len() - 1].join("\n")
```
If `lines` has exactly 2 lines (which passes the `lines.len() >= 2` check), `lines.len() - 1` is 1. The range becomes `1..1`, which yields an empty slice, destroying the payload entirely instead of preserving the content.

**Minimal Reproducible Scenario:**
Pass a string ````\n``` ` to `clean_json_payload`. It will return an empty string instead of preserving any single-line content that might be formatted strangely, or it might incorrectly parse empty blocks. Wait, if it's ````json\n{}``` `, it's 3 lines. But if it's ````{}``` ` (2 lines), it returns empty.

**Severity:** Medium (Data Loss in JSON Parsing)

**Suggested Fix:**
```rust
if lines.len() > 2 {
    lines[1..lines.len() - 1].join("\n")
} else {
    // handle 2-line case appropriately
}
```
