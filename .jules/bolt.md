## 2024-05-30 - Bottleneck in Graduation Pipeline
**Learning:** Recalculating the Euclidean norm inside an O(N^2) double loop in `graduation_pipeline.rs` causes extreme performance degradation, particularly with high-dimensional 1536d embeddings.
**Action:** Lift the norm calculation into a separate pre-processing loop and attach the pre-calculated norm to a struct, then loop through that structured data.
