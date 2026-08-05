---
labels: ['bug', 'agent-found']
severity: High
---

# Exponential graph traversal queue growth causing potential DoS

**File:** `mythrax-core/src/db/crud_operations.rs`
**Line Numbers:** 2568-2665

## Description
In `query_symbolic_scored_db`, the temporal expansion graph traversal relies on a `LIMIT 50` constraint per hop level and a depth limit. However, this creates an unbounded recursion risk.

## Minimal Reproducible Scenario
An adversarial user can craft a densely connected memory graph. With a `LIMIT 50` branching factor at each hop, a depth-3 traversal could yield up to `50^3 = 125,000` nodes in the queue, consuming excessive memory and CPU, leading to potential denial-of-service. The current Sliding Window Cap and `LIMIT 50` constraints per hop level are insufficient to prevent this.

## Suggested Fix
Implement a strict global upper bound on the maximum number of visited nodes or queue size across the entire traversal, in addition to the hop limit, to ensure bounded memory and execution time.
