## 2024-05-13 - [Zero-allocation in-place trim_end]
**Learning:** `*string = string.trim_end().to_string();` causes unnecessary heap allocations. Using `string.truncate(string.trim_end().len());` avoids this.
**Action:** Replace `*string = string.trim_end().to_string();` with `string.truncate(string.trim_end().len());` in hot paths.
