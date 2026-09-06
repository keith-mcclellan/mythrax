## YYYY-MM-DD - [Optimizing String Concatenation]
**Learning:** In Rust codebases like mythrax-core, using `string.push_str(&format!(...))` causes unnecessary temporary heap allocations for every format operation.
**Action:** Optimize string concatenation by using `std::fmt::Write` and the `write!` macro to append formatted data directly into a mutable string buffer.
