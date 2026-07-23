## 2024-07-23 - [Initial Setup]
**Learning:** Initial setup of bolt.md as requested.
**Action:** Keep this file around for critical learnings.
## 2024-07-23 - [O(N^2) Vector Normalization Bottleneck]
**Learning:** Found a severe performance bottleneck where Euclidean norm computations (`.sum::<f32>().sqrt()`) were being executed inside inner loops for high-dimensional arrays (like 1536d embeddings) when evaluating similarities in `mythrax-core/src/cognitive/synthesis.rs`. This creates quadratic growth in calculation overhead.
**Action:** When performing local clustering or evaluating cosine distance for numerous candidates in a vector space, always extend candidate representation structures (like `GradCandidate`) to cache their vector magnitude/norm on instantiation so it becomes an O(N) pre-calculation, rendering the inner loop distance calculation O(1) in metadata retrieval.
