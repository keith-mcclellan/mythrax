## 2024-07-18 - Hoisting Euclidean Norm Computations

**Learning:** Recalculating constants like Euclidean norm inside inner execution loops for high-dimensional arrays (like 1536d embeddings) causes severe O(N^2) latency bottlenecks in functions like `dbscan` and max pairwise distance calculations. A simple struct wrapping the precomputed norms can yield massive performance improvements for these distance loops.
**Action:** When calculating pairwise distances or executing nested distance iterations for embeddings, explicitly precompute and cache magnitudes or Euclidean norms outside of the nested loops. Use `CachedDistances` or similar structures to avoid O(N^2) latency cliffs.
