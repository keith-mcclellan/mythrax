# Title: ⚡ Bolt: Precalculate embedding norms for DBSCAN optimization

## Description

**💡 What:**
Created a new mathematical function `cosine_similarity_precomputed` that accepts vectors alongside their precomputed norms. Updated the `dbscan` algorithm in `synthesis.rs` to compute the norms for all input embeddings exactly once before executing the nested clustering loops.

**🎯 Why:**
During hierarchical clustering (DBSCAN), the standard `cosine_similarity` function was repeatedly recalculating the Euclidean norm of high-dimensional vectors (like 1536d embeddings) within the inner O(N²) execution loop. This redundant recalculation of constants created a severe latency bottleneck as the dataset scaled.

**📊 Impact:**
Eliminates O(N²) redundant square root and dot product calculations for the norm components. This results in significantly faster embedding clustering and reduces CPU utilization during cognitive synthesis.

**🔬 Measurement:**
This optimization can be verified by benchmarking the time taken by the `dbscan` function with increasing embedding counts (e.g. N > 1000) and observing the latency drop. No breaking changes were introduced, and tests (`math::tests` and `cognitive::synthesis::tests`) verify that the numerical outputs remain perfectly identical.
