## 2026-08-05 - Precalculate Vector Norms in Synthesis Loop
**Learning:** Recalculating Euclidean norms inside inner execution loops for high-dimensional arrays (like 1536d embeddings) causes severe O(N^2) latency bottlenecks during embedding similarity and clustering cross-scope phases.
**Action:** When performing thousands of dot product similarity checks (e.g., in graduation loops or DBSCAN), hoist heavy vector operations outside nested loops and lazily cache them by precomputing properties like `embedding_norm` on intermediate structs (e.g., `GradCandidate`).
