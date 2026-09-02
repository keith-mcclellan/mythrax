## 2024-05-24 - Optimize string concatenation
**Learning:** Using `String::push_str(&format!(...))` causes unnecessary heap allocations because `format!` creates a temporary `String`. Using `write!(string, ...).unwrap()` with `std::fmt::Write` directly formats into the target string.
**Action:** Use `write!` macro with `std::fmt::Write` instead of `push_str(&format!(...))` for building large strings in Rust.
