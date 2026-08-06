# Unbounded Recursion Risk in Temporal Expansion Graph (Depth-3 DOS)

**Labels**: `architecture-review`, `adversarial`

## Finding
The temporal expansion graph queries (e.g., `LIMIT 50` at depth 1, 2, and 3) risk bounded explosion, yielding up to 125,000 nodes without a global budget.

## Current Assumption
Applying a local `LIMIT 50` at each traversal hop is sufficient to prevent denial-of-service and memory exhaustion.

## Attack Scenario
An adversary creates dense clusters of meaningless episodic memories with highly interconnected temporal relations. When the system performs a temporal neighbor expansion (e.g., for hybrid search or compaction), the query iterates 50 * 50 * 50 (125,000 nodes) generating massive CPU, SurrealDB I/O, and string allocation overhead.

## Blast Radius
System slowdown, potential OOM (memory exhaustion), and denial-of-service blocking valid cognitive tasks and prompt construction.

## Recommended Structural Change
Implement a global context budget (e.g., maximum 5,000 nodes total across all depths) and a spreading-activation decay function that short-circuits traversal before exhausting the budget, regardless of local limits.
