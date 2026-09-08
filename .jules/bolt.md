## 2024-05-18 - String format optimization
**Learning:** Using `string.push_str(&format!(...))` causes unnecessary temporary heap allocations in Rust.
**Action:** Use `std::fmt::Write` and the `write!` macro to append formatted data directly into a mutable string buffer to avoid temporary heap allocations.
