# Panic in Synthesis on Unwrapping Float Partial Cmp

**Labels:** `bug`, `agent-found`
**Severity:** HIGH

## Description
In `mythrax-core/src/cognitive/synthesis.rs` at lines 497 and 502, `dists.sort_by(|a, b| a.partial_cmp(b).unwrap());` and `k_distances.sort_by(|a, b| a.partial_cmp(b).unwrap());` are used to sort vectors of cosine distances.

Because `partial_cmp` on `f32` returns `None` when either value is `NaN`, calling `.unwrap()` on the result will panic. The `cosine_distance` computation uses dot products divided by vector norms. If the embeddings being compared contain zeros (or the vector norms are zero), `cosine_distance` will return `NaN`. This `NaN` will crash the background thread running the synthesis step when it tries to sort the distances.

## Reproducible Scenario
1. Input an episode that generates a zero-length embedding, or inject a mocked zero-vector embedding for a cluster of episodes.
2. The `dbscan_insight_compaction` runs the elbow-point calculation using a sample of 100 embeddings.
3. `cosine_distance` compares the zero vectors and yields `NaN`.
4. `dists.sort_by` panics on the `.unwrap()`.

## Suggested Fix
Gracefully handle `NaN` sorting by falling back to `std::cmp::Ordering::Equal`.
Update the code to:
```rust
dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
k_distances.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
```
