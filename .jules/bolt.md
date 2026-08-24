## 2024-05-24 - Personalized PageRank HashMap Bottleneck
**Learning:** Iterative graph operations like PageRank perform exceptionally poorly when relying on string cloning and HashMap lookups in tight loops. This is a critical pattern in this codebase's architecture.
**Action:** Map node identifiers to contiguous integers and use flat vectors (Vec) for inner loop state rather than HashMaps to drastically reduce overhead.
