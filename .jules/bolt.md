## 2024-05-24 - Caching Euclidean Norms in Embedding Similarity
**Learning:** When calculating embedding similarity (cosine similarity via dot product divided by Euclidean norms), recalulating the norms `norm_u` and `norm_v` inside nested loops (O(N^2) complexity) creates severe latency bottlenecks, especially for high-dimensional arrays like 1536d embeddings.
**Action:** Always precalculate and cache Euclidean norms outside of execution loops for high-dimensional arrays and store them in the associated structures (e.g., adding a `norm: f32` field to a `GradCandidate` struct).
