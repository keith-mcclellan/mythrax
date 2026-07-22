---
labels: bug, agent-found
---

# Panic on `.unwrap()` when saving LLM response to DB in `synthesis.rs`

## Description
After fetching a routed completion from the LLM, the system attempts to save the updated node to the database using `db.save_wiki_node(&updated_node).await.unwrap()`. If the database operation fails (e.g., due to a local RocksDB/SurrealKV lock acquisition failure, which is common in high-concurrency environments), the thread will panic, crashing the daemon and potentially corrupting the active workflow state.

## Location
- File: `mythrax-core/src/cognitive/synthesis.rs`
- Line: 2169

## Minimal Reproducible Scenario
1. Start the daemon with multiple parallel cognitive synthesis tasks.
2. Artificially induce a database lock (e.g., locking the file or executing a heavy transaction concurrently).
3. The LLM completion succeeds, and `db.save_wiki_node` is called.
4. The database returns a lock timeout or other error.
5. The `.unwrap()` on the Result triggers a panic.

## Severity
High - Realistic trigger path during heavy load resulting in daemon crash.

## Suggested Fix
Use the `?` operator or pattern matching to gracefully bubble up the error or log a warning and retry, instead of panicking:
```rust
if let Err(e) = db.save_wiki_node(&updated_node).await {
    tracing::error!("Failed to save updated wiki node: {:?}", e);
    // Continue or return error gracefully
}
```