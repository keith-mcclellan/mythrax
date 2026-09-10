## 2025-02-12 - [Optimized search_pipeline.rs append_related_context]
**Learning:** Using `write!` with `std::fmt::Write` on a String buffer is much faster than repeatedly calling `string.push_str(&format!(...))`, as the latter performs an unnecessary string allocation just to concatenate it.
**Action:** Always prefer `write!` macro when formatting strings dynamically into an existing mutable String buffer.
