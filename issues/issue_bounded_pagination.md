---
tags: [architecture-review, adversarial]
---
# Finding: Bounded Pagination (`LIMIT 50`) in Temporal Expansion

**Current Assumption:** Enforcing `LIMIT 50` at each hop limits graph expansion and prevents OOM.

**Attack Scenario:** Adversarial memory graphs can bypass this by creating high-density connections at exactly the limit per hop. A depth-3 traversal with `LIMIT 50` yields 125,000 nodes per query, causing compute starvation and unbounded recursion.

**Blast Radius:** Denial of Service (DoS) of the compactor and retrieval services, CPU exhaustion, and query timeouts for all users.

**Recommended Structural Change:** Implement global budget constraints per query (e.g., max 1000 total nodes traversed), rather than naive per-hop limits.
