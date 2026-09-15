## 2024-05-20 - String Formatting Memory Allocations
**Learning:** In Rust codebases like mythrax-core, using `string.push_str(&format!(...))` causes unnecessary temporary heap allocations which can slow down loops over large result sets.
**Action:** Use `std::fmt::Write` and the `write!` macro to append formatted data directly into a mutable string buffer, rather than using `string.push_str(&format!(...))`.
