# Panic in Compactor on Unwrapping Episode ID and Embedding

**Labels:** `bug`, `agent-found`
**Severity:** HIGH

## Description
In `mythrax-core/src/cognitive/compactor.rs`, there are multiple instances where `.unwrap()` is called on `.id` and `.embedding` of an `Episode` within the `delta_compact_checkpoints` method.

While the code initially filters `active_eps` to ensure these fields are `Some`, the subsequent nested loop uses `.unwrap()` at lines 159, 165, 176, 177, 253, and 257. If there is any logic that mutates the entries in `active_eps` to `None` between the filter and the iteration (for instance, clearing an ID to mark deletion without removing from the collection), or if the filter condition is relaxed in the future without fixing downstream assumptions, this will panic and crash the background process.

## Reproducible Scenario
1. Implement a future logic step in compaction that mutates `active_eps[k].id = None` instead of deleting the item from `deleted_ids`.
2. Run the `delta_compact_checkpoints` logic.
3. The iteration processes `k` and hits `active_eps[k].id.as_ref().cloned().unwrap()`, crashing the background job.

## Suggested Fix
Extract the `id` and `embedding` safely. Instead of using `.unwrap()`, use `if let Some(ref id) = active_eps[i].id` or `match` and gracefully `continue` if `None`.
For example:
```rust
let id_i = if let Some(ref id) = active_eps[i].id { id.clone() } else { continue; };
```
And similarly for `id_j` and the embeddings.
