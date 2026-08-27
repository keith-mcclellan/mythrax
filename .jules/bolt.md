## 2024-08-27 - Performance-critical iterative graph operations
**Learning:** In `mythrax-core`, performance-critical iterative graph operations (such as PageRank calculations) should avoid string cloning and HashMap lookups in tight loops by mapping node identifiers to contiguous integers and using flat vectors (`Vec`) for inner loop state.
**Action:** Optimize `calculate_ppr_weights` in `mythrax-core/src/cognitive/governor.rs` to map strings to integer indices and use vectors instead of HashMaps for node states during the iterative computation.
