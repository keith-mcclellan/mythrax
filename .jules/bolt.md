## 2024-05-18 - [Optimize string concatenation in search_pipeline.rs]
**Learning:** Found unnecessary heap allocations being made via `push_str(&format!(...))` when generating related context strings for search results in `mythrax-core/src/db/search_pipeline.rs`.
**Action:** Replace `push_str(&format!(...))` with `write!(string_buf, ...).expect("write failed")` using the `std::fmt::Write` trait, avoiding the intermediate string allocation, as well as optimizing `.trim_end().to_string()` with `.truncate(trim_end().len())`.
