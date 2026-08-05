---
labels: architecture-review, adversarial
---
# Adversarial Review: Unbounded recursion risk in Temporal Expansion Graph (Orchestration Risk)

## Finding
In `mythrax-core` (specifically `db/crud_operations.rs`), the use of Sliding Window Caps (e.g., 1,000-element `VecDeque`) and `LIMIT 50` constraints per hop level during temporal expansion graph traversals creates an unbounded recursion risk. A depth-3 traversal can yield 125,000 nodes, leading to potential denial-of-service via adversarial memory graphs.

## Current Assumption
The assumption is that a fixed `LIMIT 50` per node expansion is sufficient to prevent unbounded memory growth and graph explosion.

## Attack Scenario
An attacker creates a dense, highly interconnected set of episodic memories. When the system performs a temporal expansion graph traversal (e.g., depth 3), the query expands exponentially ($50^3 = 125,000$ nodes), consuming excessive CPU and memory resources, causing a denial of service.

## Blast Radius
Denial of service for the memory retrieval system. The daemon may crash due to out-of-memory errors or CPU exhaustion.

## Recommended Structural Change
Implement a global threshold on the total number of nodes visited during graph traversal (e.g., max 1,000 nodes total), regardless of the per-hop limit. Use a priority queue to explore the most relevant edges first, rather than a naive breadth-first expansion.