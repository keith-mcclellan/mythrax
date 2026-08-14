---
title: Manual index manipulation in forge section batching
labels: bug, agent-found
---

**File:** `mythrax-core/src/cognitive/forge.rs`
**Lines:** 288, 292

**Bug Type:** Off-by-one / Logic Error

**Description:**
```rust
let end = batch[batch.len() - 1].end_byte;
```
When processing a batch of logical sections, the title is formed by taking `batch[0].title` and `batch[batch.len() - 1].title`. The `end_byte` is also taken from `batch[batch.len() - 1]`. While technically safe if `batch` is guaranteed non-empty (which we assume since `batch[0]` is accessed prior), it's idiomatic and safer to use `.last().unwrap()` or `.last().map(|x| x.end_byte)`.

More importantly, in `114`:
```rust
let end_byte = if i + 1 < current_entries.len() {
    current_entries[i + 1].1
} else {
    content.len()
};
```
This is a manual windowing operation. `windows(2)` would be much safer and avoid manual indexing.

**Minimal Reproducible Scenario:**
Pass an empty `batch` to the function (if possible). It will panic at `batch[0]`.

**Severity:** Low (Code quality / panic risk on edge cases)

**Suggested Fix:**
Use `.last().unwrap()` instead of `batch.len() - 1`. Check `if batch.is_empty()` early.
