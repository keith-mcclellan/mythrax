# ⚡ Bolt: [performance improvement]

## 💡 What
Optimized the `dbscan` implementation in `mythrax-core/src/cognitive/synthesis.rs` by hoisting and lazily caching the calculation of Euclidean norms.

## 🎯 Why
Calculating Euclidean norms inside inner execution loops for high-dimensional arrays (like 1536d embeddings in `mythrax-core`'s DBSCAN) causes severe O(N^2) latency bottlenecks. The `cosine_distance` calls within `find_neighbors` were recalculating the norms for the embeddings every single time.

## 📊 Impact
Precalculating the norms of embeddings outside the O(N^2) DBSCAN nested loops prevents recalculating redundant math operations for every neighbor check, drastically reducing the required compute power for calculating distances during embedding similarity and clustering, especially in vectors of 1536 dimensions. This changes the O(N^2) operation to fetch Euclidean norms, to O(N).

## 🔬 Measurement
Run `MYTHRAX_TEST_MOCK=1 cargo test --lib cognitive::synthesis -- --test-threads=1` and ensure tests pass. For measuring impact, benching the `dbscan` process against a reasonably large sample size of 1536d vectors will reveal the time scaling differences.
