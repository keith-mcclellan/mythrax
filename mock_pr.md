## ⚡ Bolt: Eliminate heap allocations in text processing loops

### 💡 What
Replaced usages of `current.clone()` followed immediately by `current.clear()` (or variable reassignment) with `std::mem::replace(&mut current, String::new())` (or `String::with_capacity(max_chars)`) in tight loops handling chunking and CLI parsing within `mythrax-core/src/vault/ingestion.rs` and `mythrax-core/src/cognitive/executor.rs`.

### 🎯 Why
In Rust, `String::clone()` performs a deep copy of the string data, leading to O(N) heap allocations. Since these arrays and variables were immediately being cleared or reset to zero-capacity defaults (which causes unnecessary allocations when data is next pushed to them), `std::mem::replace` allows us to securely transfer ownership of the existing memory buffer directly into the target `Vec` and swap a fresh, optionally pre-sized buffer in its place without any cloning overhead. This avoids dual allocations and increases processing speed in the ingestion logic.

### 📊 Impact
- Eradicates O(N) memory allocations per chunk/arg in `split_by_chars`, `group_sub_chunks`, and the cognitive executor argument parser.
- Minimizes CPU time spent mapping, allocating, and freeing unused temporary string buffers.
- Improves memory throughput on large document parsing.

### 🔬 Measurement
Run `MYTHRAX_TEST_MOCK=1 cargo test --lib --bins` to ensure the exact same parsing and chunking behavior is maintained, which verifies safety and correctness. Profiling tools like `samply` or `cargo-flamegraph` on large document ingestion will reveal reduced time spent inside `__memcpy` and `malloc` originating from these functions.