# [Red Team] Cognitive Pipeline: Unbounded Recursion via DBSCAN Compaction

**Labels:** architecture-review, adversarial

**Finding:** The memory compaction pipeline (`cognitive/compactor.rs`) uses DBSCAN clustering and hierarchical RAPTOR synthesis. The `ARCHITECTURE.md` asserts that queries avoid unbounded calls via `LIMIT 50` and temporal expansion has a max depth. However, the sliding window cap and batch constraints do not prevent logical recursion in the synthesis phase.

**Current Assumption:** Limiting batch sizes and using `LIMIT 50` paginated queries naturally prevents unbounded memory expansion during the "daily dreaming compactor" cycles.

**Attack Scenario:**
1. An attacker feeds a sequence of highly interconnected, cyclically referencing memory nodes (e.g., Node A relates to Node B, B to C, C to A).
2. During the DBSCAN clustering and RAPTOR summary generation, the system attempts to synthesize summaries of these clusters.
3. If the generated summary structurally resembles the input nodes closely enough, it will be fed back into the next day's clustering cycle.
4. Because temporal neighbor expansion traverses `followed_by` edges (up to depth 3), the agent could trigger a cascading loop of summary generations that continuously push the 500-node LRU cap, evicting all legitimate project memory in favor of the recursive loop artifacts.

**Blast Radius:**
- Complete loss of short-term and project-level memory (amnesia) due to cache pollution.
- High CPU/GPU thrashing as the compactor constantly reruns inference on the same recursive clusters.

**Recommended Structural Change:**
- Implement a strict cyclic dependency detection mechanism during cognitive expansion.
- Add a cryptographic hash verification of cluster generation provenance to ensure that a cluster's synthesized output cannot form a closed loop with its parents.
- Require an ADR defining the exact limits on hierarchical summary depths and recursion prevention.