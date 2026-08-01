## 2024-08-01 - Precomputing Euclidean Norms for O(N^2) Similarity
**Learning:** Recalculating Euclidean norms inside inner execution loops for high-dimensional arrays causes severe latency bottlenecks.
**Action:** When finding `cosine_similarity` in an inner loop, hoist the `norm` computations out of the loop into a precomputed struct (like `GradCandidate`).
