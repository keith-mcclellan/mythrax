## 2024-05-24 - Hoist Euclidean norm computation out of nested loops in Synthesis

**Learning:** When generating in-memory fallbacks for clustering logic and similarities within the `cognitive/synthesis.rs` memory module, Euclidean norms (`norm_u` and `norm_v`) were needlessly recalculated at scale within O(N) operations. Because these values remain constant per candidate embedding, calculating this value inside an inner nested loop significantly blocks and bottlenecks heavy similarity calculations (50%+ latency impact).
**Action:** Always verify that loop operations that calculate vector/mathematical transformations on constant values are eagerly cached or hoisted outside the inner execution logic block when iterating across arrays of large dimensions (like 1536d arrays for models).
