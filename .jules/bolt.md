## 2024-07-21 - [DBSCAN Bottleneck in High-Dimensional Embeddings]
**Learning:** Recalculating Euclidean norms (`norm_u` and `norm_v`) inside inner execution loops for high-dimensional arrays (like 1536d embeddings) causes severe O(N^2) latency bottlenecks.
**Action:** Hoist vector operations like Euclidean norm computations outside of nested loops and lazily cache them when working with embedding similarity and clustering in mythrax-core.
