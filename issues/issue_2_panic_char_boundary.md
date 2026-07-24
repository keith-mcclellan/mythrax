---
title: "Bug: String Slicing Panic via Character Boundary Violation"
labels: ["bug", "agent-found"]
---

**File:** `mythrax-core/src/db/backend.rs`
**Line:** ~836

**Description:**
In `compact_search_result`, a binary search determines the character limit for string truncation. It naively slices the string using `&original_content[..mid]`. Because Rust strings are UTF-8 encoded, if `mid` falls inside a multi-byte character (like an emoji or non-ASCII text), the slice operation will panic with a "byte index is not a char boundary" error.

**Minimal Reproducible Scenario:**
Processing search results containing multi-byte characters and attempting to truncate them directly in the middle of a multi-byte encoding.

**Severity:**
High (Crash during text truncation of external or generated content)

**Suggested Fix:**
Before slicing, safely decrement the `mid` index down to the nearest valid char boundary using `while mid > 0 && !original_content.is_char_boundary(mid) { mid -= 1; }`.
