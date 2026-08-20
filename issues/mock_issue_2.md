# Bug: Panic in Blackboard try_send Expect

**File:** `mythrax-core/src/db/blackboard.rs`
**Line:** 98, 105

## Minimal Reproducible Scenario
In `mythrax-core/src/db/blackboard.rs`, the `try_send` calls for `tx` are followed by `.expect("First send should succeed")` and `.expect("Second send should succeed")`. If the channel is unexpectedly full or closed under high concurrency or test environments, these paths will panic.

## Severity
High - Directly crashes the blackboard process on message send failures.

## Suggested Fix
Properly handle the `TrySendError`, returning an error or retrying, instead of using `.expect()`.