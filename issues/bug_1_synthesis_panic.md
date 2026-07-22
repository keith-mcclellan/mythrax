---
labels: bug, agent-found
---

# Panic on `.unwrap()` during distance sorting in `synthesis.rs`

## Description
When calculating DBSCAN elbow points or finding K-nearest distances during memory synthesis, the code sorts distances using `partial_cmp().unwrap()`. Floating-point comparisons can result in `None` if `NaN` values are present (which can occur if an embedding vector has zero length or during identical comparisons resulting in division-by-zero somewhere in the math library if not carefully checked, though usually it's just NaN propagation). Unwrapping a `None` result from `partial_cmp` causes a panic, which crashes the process.

## Location
- File: `mythrax-core/src/cognitive/synthesis.rs`
- Lines: 631 and 636

## Minimal Reproducible Scenario
1. Insert two nodes with malformed or zero-length embeddings that result in a distance of `NaN`.
2. Trigger a dreaming compaction loop that includes these nodes.
3. The `dists.sort_by(|a, b| a.partial_cmp(b).unwrap())` line is reached.
4. `a.partial_cmp(b)` returns `None` due to `NaN` comparison.
5. `.unwrap()` panics, bringing down the daemon.

## Severity
High - Crashes the main or background cognitive task thread.

## Suggested Fix
Replace `.unwrap()` with a safe fallback, such as:
```rust
dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
```
Additionally, validate that vectors are non-zero length and normalize properly before computing similarities.