# Bolt's Journal
## 2024-07-25 - O(N^2) Bottleneck in High-Dimensional DBSCAN Clustering
**Learning:** Recalculating Euclidean norm constants (like `norm_u` and `norm_v`) inside inner execution loops for high-dimensional arrays (like 1536d embeddings in `mythrax-core`'s DBSCAN implementation) causes severe O(N^2) latency bottlenecks. The `cosine_distance` calls within `find_neighbors` recalculate the norms every time.
**Action:** Always hoist heavy vector operations like Euclidean norm computations outside of nested loops and lazily cache them before executing O(N^2) similarity scans across large datasets.
