---
labels: architecture-review, adversarial
---
# Finding: Unbounded Recursion via Temporal Expansion Graph Constraints

**Current Assumption:** Applying a `LIMIT 50` constraint per hop level during temporal expansion graph traversals bounds computational complexity and memory usage.

**Attack Scenario:** An attacker crafts an adversarial memory graph with dense interconnectivity. A depth-3 traversal using a `LIMIT 50` branching factor yields up to 125,000 nodes (50^3), leading to catastrophic unbounded recursion and OOM during context aggregation.

**Blast Radius:** Denial of Service (DoS) of the retrieval pipeline and daemon crashes due to memory exhaustion. This represents a severe orchestration risk.

**Recommended Structural Change:** Replace per-hop limits with an absolute global cap on total traversed nodes (e.g., max 500 nodes total across all depths) and enforce a cycle-detection visited set during graph expansion.

*Note: Never close this issue without a documented architectural decision record (ADR) response.*
