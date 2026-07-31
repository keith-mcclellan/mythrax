## 2024-07-31 - Precomputing High-Dimensional Norms to Avoid O(N^2) Bottlenecks
**Learning:** Recalculating Euclidean norms inside inner execution loops for high-dimensional arrays (like 1536d embeddings) causes severe O(N^2) latency bottlenecks during embedding similarity and clustering, particularly in the graduation cross-scope clustering passes.
**Action:** Heavy vector operations must be hoisted outside nested loops and lazily cached (e.g., precomputing `embedding_norm` on structs like `GradCandidate` upon instantiation).
