## 2024-05-18 - String Concatenation Bottleneck
**Learning:** In mythrax-core, `push_str(&format!(...))` is frequently used for markdown generation, which causes temporary heap allocations that slow down processing.
**Action:** Always use `std::fmt::Write` and `write!(&mut buffer, ...)` to avoid temporary allocations when formatting strings in hot paths.
