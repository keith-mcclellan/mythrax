# Unbounded Recursion Risk in Temporal Graph Traversals (`LIMIT 50` Hop explosion)

**Labels:** architecture-review, adversarial

**Finding:** Temporal expansion graph traversals apply `LIMIT 50` constraints per hop level (e.g., in `mythrax-core/src/db/crud_operations.rs`). A depth-3 traversal can yield 50^3 (125,000) nodes.

**Current Assumption:** Capping queries at `LIMIT 50` per hop is sufficient to prevent unbounded memory growth during cognitive synthesis and temporal traversal.

**Attack Scenario:** An adversary (or misbehaving agent) creates a highly dense temporal graph by interlinking hundreds of episodic memories. When a depth-3 search traversal is triggered, the system attempts to process 125,000 nodes, exhausting memory and compute resources.

**Blast Radius:** Denial of Service via memory exhaustion (OOM) or extreme latency bottlenecks during retrieval and clustering, crashing the single daemon instance.

**Recommended Structural Change:** Implement an absolute global limit on the number of nodes visited during graph traversals (e.g., max 500 nodes total, regardless of depth) and introduce a visited-node cache to prune duplicate paths early.
