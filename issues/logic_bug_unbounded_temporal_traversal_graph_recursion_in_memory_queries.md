---
title: Logic Bug: Unbounded temporal traversal graph recursion in memory queries
labels: bug, agent-found
---

**File & Line:** `mythrax-core/src/cognitive/synthesis.rs:3435-3460` (Graph traversal from `cluster` node ids fanning out to related memory nodes via `db.get_related_node_ids`)

**Minimal Reproducible Scenario:** In temporal synthesis or related node queries, temporal expansion traverses hops with a sliding window cap limit (e.g. 50). A deep query fanning out across many heavily interlinked memories will cause an uncontrolled explosion of visited nodes per-level in memory queries. This results in heavy processing latency, database thrashing, and potential memory exhaustion DoS when traversing adversarially constructed cognitive graphs.

**Severity:** High (Resource Exhaustion/DoS)

**Suggested Fix:** Impose a hard, global limit on the total number of unique nodes processed across an entire traversal operation, maintaining a global counter rather than just limiting per-hop branching, and terminating the traversal early if the limit is exceeded.