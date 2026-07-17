# ⚡ Bolt: Prevent O(N^2) latency bottleneck by precomputing Euclidean norms

### 💡 What
Refactored the cognitive synthesis clustering and graduation candidate comparisons in `mythrax-core/src/cognitive/synthesis.rs`. Added a `norm: f32` property to the `GradCandidate` struct to precompute and cache the Euclidean norm of high-dimensional embeddings. Replaced inner loop calculations with referencing the cached variables.

### 🎯 Why
Calculating `.iter().map(|x| x * x).sum::<f32>().sqrt()` on 1536-dimensional embeddings inside nested iteration loops scales extremely poorly (O(N^2)). Pre-computing the values outside these inner loops reduces repeated mathematical calculations overhead down to O(1) inside the loop constraints.

### 📊 Impact
Expected to significantly lower CPU utilization and memory overhead during cross-scope graduation indexing and HNSW fallbacks where numerous similarity checks are required, vastly speeding up execution times.

### 🔬 Measurement
Benchmark CPU cycles parsing memory graduations using standard metrics, observing reduction in block times.
