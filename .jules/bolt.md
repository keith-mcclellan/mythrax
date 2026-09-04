## 2024-05-15 - [String concatenation in mythrax-core]
**Learning:** In Rust codebases like mythrax-core, optimize string concatenation by using `std::fmt::Write` and the `write!` macro to append formatted data directly into a mutable string buffer, rather than using `string.push_str(&format!(...))` which causes unnecessary temporary heap allocations.
**Action:** Use `use std::fmt::Write;` and `write!(string, ...)` for string concatenations when appropriate.
