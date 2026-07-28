## 2024-05-24 - Precomputing Embedding Norms
**Learning:** In `mythrax-core`, recalculating Euclidean norms inside inner execution loops for high-dimensional arrays (like 1536d embeddings) causes severe O(N^2) latency bottlenecks during embedding similarity and clustering.
**Action:** Always hoist heavy vector operations outside nested loops and lazily cache them (e.g., precomputing `embedding_norm` on structs like `GradCandidate`).
