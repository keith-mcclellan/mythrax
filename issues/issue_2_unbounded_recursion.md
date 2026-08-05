---
title: "Adversarial CTO: Unbounded Recursion / DoS in Temporal Expansion Graph"
labels: ['architecture-review', 'adversarial']
---

## Finding
Unbounded Recursion / DoS in Temporal Expansion Graph

## Current Assumption
Bounded pagination (`LIMIT 50` per hop) on temporal expansion graph traversals (e.g., in `db/crud_operations.rs`) sufficiently prevents unbounded recursion and memory explosion.

## Attack Scenario
An attacker crafts or naturally triggers the creation of dense, highly interconnected memory clusters. Because the `LIMIT 50` constraint is only applied *per hop level*, a depth-3 traversal yields $50 \times 50 \times 50 = 125,000$ nodes. The agent attempts to process this massive context in a single cognitive sweep, causing severe CPU starvation and memory exhaustion.

## Blast Radius
Denial of Service (DoS) for the entire Mythrax daemon due to memory explosion during memory expansion, stalling all concurrent processing and crashing the node.

## Recommended Structural Change
Implement an absolute global cap on the total number of traversed nodes across all hops (e.g., max 500 nodes total), not just per level. Apply graph decay algorithms to prioritize nodes and prune the search space early. Never close this issue without a documented ADR response.
