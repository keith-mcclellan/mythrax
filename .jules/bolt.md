## 2024-07-26 - Hoisting Vector Norms in Rust DBSCAN
**Learning:** Recalculating Euclidean norms within O(N^2) inner loops (like high-dimensional embedding clustering in DBSCAN) causes severe performance bottlenecks in Rust.
**Action:** Always hoist invariant scalar computations (like `.map(|&x| x * x).sum::<f32>().sqrt()`) out of nested loops and precompute them in an O(N) pass for O(1) lookups during pairwise similarity checks.
