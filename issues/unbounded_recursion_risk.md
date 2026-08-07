---
title: Unbounded recursion risk in temporal expansion graph traversal (crud_operations.rs)
labels: bug, agent-found
---

**File:** `mythrax-core/src/db/crud_operations.rs`
**Line:** 2572

**Minimal Reproducible Scenario:**
In the function `temporal_expansion_graph_traversal`, a `VecDeque` is used with a `LIMIT 50` query per hop and a depth bound of up to `max_depth` (default 3). While there is a check `if hits.len() >= 1000 { break; }`, it only bounds the output `hits`, it does not strictly bound the memory size of `queue`. If a user injects an adversarial memory graph with dense connections, a single traversal node expansion could push up to 50 nodes per level. Without deduplication in the `queue`, the traversal will process identical nodes multiple times, compounding to `50^3 = 125,000` queue elements for depth 3, and potentially vastly more for higher depths.

**Severity:** High (Denial of Service via adversarial memory graphs causing CPU and memory exhaustion).

**Suggested Fix:**
Maintain a `visited` HashSet (or check `path_conf`) to avoid revisiting nodes, and explicitly bound the maximum number of items that can be pushed to the `queue` or the max number of nodes processed across the entire traversal loop.
