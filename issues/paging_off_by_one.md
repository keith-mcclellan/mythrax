# Bug (agent-found): Potential off-by-one and slice panic in `paging.rs`

**File:** `mythrax-core/src/cognitive/paging.rs`
**Line:** 24 & 39 & 56

**Description:**
In `match_symbol`, the code calculates lengths and uses slicing like `clean_remaining[..len]`. If `clean_remaining` contains multi-byte UTF-8 characters, `len` based on `chars().count()` is the number of characters, not bytes. Slicing `&str` by character count rather than byte indices will cause a panic at runtime (`byte index x is not a char boundary`).

**Severity:** High (Crash / DoS)

**Minimal Reproducible Scenario:**
Provide a rust file to `page_code_block` that has a struct/fn name containing a multi-byte unicode character or emoji, e.g., `pub fn test_ƒ() {}`. The character count will not match the byte index, causing `clean_remaining[..len]` to panic.

**Suggested Fix:**
Use `.char_indices()` to find the byte offset, or use a proper Regex/Parser instead of manual string slicing by char count.
```rust
let byte_len = clean_remaining
    .char_indices()
    .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
    .last()
    .map(|(i, c)| i + c.len_utf8())
    .unwrap_or(0);
```
