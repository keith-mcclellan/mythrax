# ⚡ Bolt: [performance improvement]

💡 What: Hoisted the Euclidean norm calculation out of the O(N^2) inner execution loop in `dbscan` within `mythrax-core/src/cognitive/synthesis.rs`. Precomputed norms in an O(N) pass and passed them to an inline cosine similarity computation in `find_neighbors`.

🎯 Why: Recalculating constants like `norm_u` and `norm_v` inside inner execution loops for high-dimensional arrays (like 1536d embeddings) causes severe O(N^2) latency bottlenecks.

📊 Impact: Reduces computational overhead significantly by doing O(1) norm lookups instead of repeatedly iterating over arrays of size 1536.

🔬 Measurement: Compile using `cargo check` and run `cargo test --lib cognitive::synthesis` to verify correct clustering behavior is preserved.
