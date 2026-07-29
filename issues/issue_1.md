---
title: Panic risk when binary searching character truncation in backend.rs
labels: bug, agent-found
severity: High
---

**File/Line:** `mythrax-core/src/db/backend.rs` : 1044

**Minimal Reproducible Scenario:**
In the inner-node compaction fallback, `mid = (low + high) / 2` calculates a byte index to truncate the string `original_content`. If `mid` falls within a multi-byte UTF-8 character, `&original_content[..mid]` will panic with "byte index is not a char boundary". This can easily be triggered when Chinese characters or emojis are present in the text being truncated.

**Suggested Fix:**
Add a check to decrement the `mid` index until it falls on a valid character boundary before slicing.
```rust
let mut safe_mid = mid;
while safe_mid > 0 && !original_content.is_char_boundary(safe_mid) {
    safe_mid -= 1;
}
let candidate_content = if safe_mid < original_content.len() {
    format!("{}... [Truncated (Inner-Node Compaction)]", &original_content[..safe_mid])
...
```
