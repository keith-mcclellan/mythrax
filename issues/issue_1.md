# Bug: Integer overflow in metric access count increment
**Labels**: bug, agent-found

**File**: `mythrax-core/src/db/crud_operations.rs`
**Line**: 2919, 2989, 3024
**Severity**: High

**Scenario**:
The `access_count` increments do not check for overflow (e.g., `row.access_count + 1`, `access_count = access_count + 1`). An unbounded integer overflow can lead to a panic in Rust (if compiled with overflow checks or in debug mode) or wraparound behavior. This metric dictates the utility score over time and could distort memory management.

**Suggested Fix**:
Use saturating addition (`row.access_count.saturating_add(1)`) in Rust code, or implement bounded updates in SurrealQL to cap the max value.