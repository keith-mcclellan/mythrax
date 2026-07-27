# ⚡ Bolt: [performance improvement] Optimize Euclidean norm computation in DBSCAN and Synthesis clustering

## 💡 What
This PR optimizes the cross-scope graduation pass in `mythrax-core/src/cognitive/synthesis.rs` by adding an `embedding_norm` field to the `GradCandidate` struct to lazy-load and cache Euclidean norms of vector embeddings. This prevents recalculating norms during the quadratic `O(N^2)` combinatorial pairwise comparison loops in `graduate_wisdom` and `save_wiki_node_with_contradiction_resolution`.

## 🎯 Why
Calculating Euclidean norms inside inner execution loops for high-dimensional arrays (like 1536d embeddings) causes severe O(N^2) latency bottlenecks.

## 📊 Impact
Expected to significantly reduce CPU cycles in synthesis clustering, leading to visibly faster cluster processing times due to fewer redundant math operations on 1536-dimensional embeddings.

## 🔬 Measurement
Verify the improvement by running the synthesis loop over a large workspace/vault with thousands of insights and measuring total wall clock time of the background synthesis.
