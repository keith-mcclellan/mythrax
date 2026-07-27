## 2024-05-24 - O(N^2) Latency Bottleneck in Vector Operations
**Learning:** When working with embedding similarity and clustering in mythrax-core, recalculating Euclidean norms inside inner execution loops for high-dimensional arrays (like 1536d embeddings) causes severe O(N^2) latency bottlenecks.
**Action:** Ensure heavy vector operations like Euclidean norm computations are hoisted outside of nested loops and precomputed for structs used in quadratic comparisons.
