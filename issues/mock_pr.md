# ⚡ Bolt: [performance improvement]

**💡 What:** Hoisted vector norm computations (`norm_u`, `norm_v`) outside of inner loops and added `norm` caching to `GradCandidate` structs.

**🎯 Why:** Recalculating constants like `norm_u` and `norm_v` inside inner execution loops for high-dimensional arrays (like 1536d embeddings) causes severe O(N^2) latency bottlenecks.

**📊 Impact:** Reduces latency complexity of norm calculations from O(N^2) to O(N) by calculating the norm only once during struct construction.

**🔬 Measurement:** Benchmark the execution time of cross-scope graduation passes or general embedding similarity computations before and after these changes; you should see measurable reductions in compute time under large volumes.