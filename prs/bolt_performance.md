# ⚡ Bolt: Caching Euclidean norms in embedding similarity

## 💡 What
Modified `mythrax-core/src/cognitive/synthesis.rs` to cache Euclidean norms of vector embeddings to avoid O(N^2) complexity in nested loops for similarity calculations.

## 🎯 Why
When working with embedding similarity and clustering (e.g., DBSCAN/RAPTOR memory compaction), heavy operations like Euclidean norm computations were occurring inside inner execution loops. For high-dimensional arrays (like 1536d embeddings), this causes severe O(N^2) latency bottlenecks. Pre-calculating these values speeds up the loop dramatically.

## 📊 Impact
Measurably faster computation for vector clustering and similarity, dramatically reducing the O(N^2) calculations of Euclidean norms by caching values in structures and variables before inner loops.

## 🔬 Measurement
Tests pass and the cognitive module functions remain behaviorally unchanged, simply running with reduced latency by avoiding recalculating `norm` on each pairwise similarity check.
