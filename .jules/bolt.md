## 2024-06-25 - Avoid String Reallocations in Text Chunking
**Learning:** In Rust text processing loops (like chunking large logs), using `.clone()` and `.clear()` on strings creates massive unnecessary heap allocations. While `std::mem::take()` is idiomatic, it replaces the string with a zero-capacity default buffer, causing unnecessary reallocation overhead on subsequent pushes.
**Action:** Always prefer `std::mem::replace(&mut current, String::with_capacity(size))` over `.clone()` or `std::mem::take()` when the string buffer will be immediately reused for similar-sized allocations.
