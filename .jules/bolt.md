## 2024-05-24 - [Avoid cloning large result vectors]
**Learning:** Found an unnecessary clone of the whole query results vector `unfiltered.results.clone()` in `mythrax-core/src/db/backend/mod.rs` inside the `search_filtered` method. The loop can consume the vector instead and drop unmatched rows, returning a newly constructed or filtered vector, avoiding an O(N) heap allocation on every search call.
**Action:** Replace `for res in unfiltered.results.clone() {` with `for res in unfiltered.results.into_iter() {` and adjust logic accordingly.
