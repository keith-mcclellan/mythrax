## 2024-10-24 - Optimizing Iterative Graph Operations
**Learning:** Performance-critical iterative graph operations like PageRank calculations suffer from high overhead when using String cloning and HashMap lookups in tight loops.
**Action:** Map node identifiers to contiguous integers and use flat vectors (Vec) for inner loop state to avoid allocations and fast lookups.
