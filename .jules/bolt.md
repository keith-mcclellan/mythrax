## 2023-10-27 - Graph Iteration Bottlenecks
**Learning:** In performance-critical graph iteration operations like PageRank calculations, using `String` cloning and `HashMap` lookups inside tight loops is a massive bottleneck.
**Action:** Map string node identifiers to contiguous integers first, and use flat vectors (`Vec`) for inner loop state, mapping back to strings only at the end.
