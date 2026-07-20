---
title: "Bug: Panic risk on byte-range slicing in cognitive/forge.rs"
labels: ["bug", "agent-found"]
---

### Description
In `mythrax-core/src/cognitive/forge.rs`, the `build_grouped_section` function slices a `&str` using raw byte indices (`content[start..end].to_string()`). If the content contains multi-byte UTF-8 characters (e.g., emojis, non-Latin text) and the TOC parsing logic produces `start` or `end` indices that do not land on character boundaries, this slicing operation will panic.

### File and Line Number
* `mythrax-core/src/cognitive/forge.rs`, line 612

### Minimal Reproducible Scenario
1. Provide a markdown document containing multi-byte UTF-8 characters (e.g., "😊 Hello") near section boundaries.
2. If the internal TOC indexer determines an `end_byte` that falls in the middle of the "😊" character bytes, `build_grouped_section` is called.
3. The slice `content[start..end]` is evaluated.
4. Rust panics with "byte index is not a char boundary".

### Severity
**Medium** - Hard crash on specifically formatted non-ASCII inputs.

### Suggested Fix
Either ensure the TOC generator strictly enforces char boundary alignment, or fall back to a safe slicing mechanism or character-based truncation instead of byte-based. A quick safeguard is checking `content.is_char_boundary(start) && content.is_char_boundary(end)`.

```rust
if content.is_char_boundary(start) && content.is_char_boundary(end) {
    content[start..end].to_string()
} else {
    // Fallback or boundary correction logic
}
```
