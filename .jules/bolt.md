## 2024-09-11 - Rust String Buffer Optimization
**Learning:** In Rust codebases, extensive use of `string.push_str(&format!(...))` inside loops creates significant performance overhead due to the temporary allocation of a completely new `String` on the heap for every `format!` call.
**Action:** Replace `string.push_str(&format!(...))` with `std::fmt::Write` trait's `write!` macro (e.g., `write!(&mut string, ...).expect("write failed")`) to write formatted content directly into the destination buffer and bypass temporary allocations.
