---
title: Logic Bug: Potential panic due to char boundary truncation in String slicing
labels: bug, agent-found
---

**File & Line:** `mythrax-core/src/db/backend.rs:1044`

**Minimal Reproducible Scenario:** When attempting to compact an inner node, the text is sliced by byte index: `&original_content[..mid]`. If the text contains multi-byte UTF-8 characters and `mid` falls within the bytes of such a character, a panic occurs: `byte index X is not a char boundary; it is inside Y (bytes A..B) of string`. The `original_content` is passed to the engine. If it comes from external content like an LLM response or a user input with emojis, it can crash the engine.

**Severity:** High (Crash/Panic)

**Suggested Fix:** Ensure slicing happens on character boundaries, e.g. using `chars().take(mid).collect::<String>()` or check `is_char_boundary(mid)` and decrement `mid` until it is.