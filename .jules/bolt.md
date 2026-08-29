## 2024-05-24 - Optimized PageRank Calculation Iteration
**Learning:** In `mythrax-core`, performance-critical iterative graph operations like PageRank suffer from heavy allocations when using `HashMap<String, f32>` and string cloning in tight loops.
**Action:** Map string identifiers to contiguous integer indices (`usize`) upfront and use flat vectors (`Vec`) for loop states to bypass costly hashing and allocation during iterations.
