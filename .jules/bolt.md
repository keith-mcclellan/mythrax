## 2024-08-04 - Optimize Cross-Project Similarity Comparison in Graduation Pipeline
**Learning:** In `mythrax-core`, recalculating Euclidean norms inside inner execution loops for high-dimensional arrays (like 1536d embeddings) causes severe O(N^2) latency bottlenecks during embedding similarity and clustering, such as in `graduation_pipeline`.
**Action:** Heavy vector operations must be hoisted outside nested loops and lazily cached. Specifically, precompute `embedding_norm` on structs like `GradCandidate` to avoid redundant norm calculations during similarity checks.
