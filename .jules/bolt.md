## 2024-08-23 - [Optimizing Mythrax PageRank calculation]
**Learning:** In mythrax-core, performance-critical iterative graph operations (such as PageRank calculations) should avoid string cloning and HashMap lookups in tight loops.
**Action:** Map node identifiers to contiguous integers and use flat vectors (`Vec`) for inner loop state to eliminate O(n^2) map lookup costs and improve computation efficiency.
