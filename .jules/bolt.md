## 2024-09-13 - [String Formatting Allocations]
**Learning:** Using `string.push_str(&format!(...))` in Rust is a common anti-pattern that creates unnecessary, short-lived `String` allocations on the heap just to format the values before appending them. Additionally, `*string = string.trim_end().to_string();` forces a reallocation.
**Action:** Use `std::fmt::Write` and the `write!(string, ...)` macro to append formatted data directly into the mutable string's existing buffer. To trim in-place without allocating, use `string.truncate(string.trim_end().len());`.
