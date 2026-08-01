# ⚡ Bolt: [performance improvement] Precompute norms in graduation pipeline

## Description
💡 What: We replaced the direct `cosine_similarity` call inside a nested loop with precomputed Euclidean norms wrapped in a `GradCandidate` struct.
🎯 Why: `run_graduation_pipeline` calculates cosine similarity in an $O(N^2)$ loop over vectors. Computing the square root and norms within the loop caused a performance bottleneck for high-dimensional structures.
📊 Impact: Expected to reduce execution time of graduation pipeline significantly when many local and other nodes are analyzed by shifting norm calculation from $O(N^2)$ down to $O(N)$.
🔬 Measurement: Verify changes using benchmark timings for the graduation pipeline or tracking CPU usage scaling against node count.
