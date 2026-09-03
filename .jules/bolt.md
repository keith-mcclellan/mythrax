## 2024-09-03 - Optimize String Concatenation in Loops
**Learning:** Using `String::push_str(&format!(...))` inside loops causes unnecessary temporary heap allocations for each formatting operation.
**Action:** Use `std::fmt::Write` and the `write!` macro to append formatted data directly into the mutable string buffer.
