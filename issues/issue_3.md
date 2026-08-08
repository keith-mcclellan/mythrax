# Bug: Unbounded Sliding Window recursion in graph traversal
**Labels**: bug, agent-found

**File**: `mythrax-core/src/db/crud_operations.rs`
**Line**: 2575, 2580
**Severity**: High

**Scenario**:
The use of Sliding Window Caps (e.g., `VecDeque` queue) and `LIMIT 50` constraints per hop level during temporal expansion graph traversals creates an unbounded recursion risk. A depth-3 traversal can yield 125,000 nodes, leading to potential denial-of-service via adversarial memory graphs.

**Suggested Fix**:
Introduce strict total node visit caps and limit maximum expansion graph edges traversed across the entire graph per user request.