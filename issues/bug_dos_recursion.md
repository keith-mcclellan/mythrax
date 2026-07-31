# Bug: Denial-of-Service vulnerability via unbounded breadth-first search queue recursion

**Labels:** bug, agent-found

**File/Line:** `mythrax-core/src/db/crud_operations.rs`, line 2634 (within `query_symbolic_scored_db`)

**Minimal Reproducible Scenario:**
During a temporal expansion graph traversal in `query_symbolic_scored_db`, the code uses an unbounded `VecDeque` for BFS graph traversal. Although depth is capped (`limit_depth`), the traversal can query up to 50 items per hop. A depth-3 traversal could yield up to 50^3 = 125,000 enqueued items. An adversarial graph topology could cause extreme memory consumption or out-of-memory crashes due to this unbounded expansion.

**Severity:** High

**Suggested Fix:**
Introduce an explicit limit on the queue length. Insert an early-exit check inside the loop to cap the queue at 1,000 items:
```rust
if queue.len() >= 1000 {
    break;
}
```