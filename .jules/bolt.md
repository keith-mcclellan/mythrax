## 2024-07-24 - Zero-Allocation String chunking with `std::mem::replace`
**Learning:** In text processing pipelines (like `vault/ingestion.rs`), doing `.clone()` followed by `.clear()` or variable reassignment causes unnecessary $O(N)$ memory allocations.
**Action:** Use `std::mem::take` and `std::mem::replace` to avoid cloning string buffers when shifting ownership into collections.
