## 2024-07-22 - Precomputing norms for dbscan
**Learning:** In mythrax-core (e.g., cognitive/synthesis.rs), recalculating Euclidean norms inside inner execution loops for high-dimensional arrays (like 1536d embeddings) causes severe O(N^2) latency bottlenecks.
**Action:** Always precompute heavy vector operations like Euclidean norms outside of nested loops and pass them to inner functions lazily.
