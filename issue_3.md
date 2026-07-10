---
title: "Bug: Panic in `synthesis.rs` during floating point sorting with NaN"
labels: ["bug", "agent-found"]
severity: "Medium"
---

## Description
In `synthesis.rs`, when finding the optimal epsilon for DBSCAN using the k-distance graph method, the code sorts an array of floating-point distances using `.sort_by(|a, b| a.partial_cmp(b).unwrap())`. If the distance calculation results in `NaN` (e.g., due to division by zero if vectors are zero-length or malformed embeddings), `partial_cmp` returns `None`, and the `.unwrap()` will panic.

## File and Line Numbers
- `mythrax-core/src/cognitive/synthesis.rs:480`: `dists.sort_by(|a, b| a.partial_cmp(b).unwrap());`
- `mythrax-core/src/cognitive/synthesis.rs:485`: `k_distances.sort_by(|a, b| a.partial_cmp(b).unwrap());`

## Minimal Reproducible Scenario
1. Provide an array of embeddings containing a malformed, zero-magnitude vector.
2. The `cosine_distance` calculation generates a `NaN` result.
3. The clustering routine attempts to sort the distances array.
4. `partial_cmp` returns `None`, triggering the panic.

## Blast Radius
The synthesis engine crashes during memory compaction/clustering, preventing insight generation.

## Suggested Fix
Use a robust float comparison that handles `NaN` values, such as the `f32::total_cmp` method available in newer Rust versions, or explicitly handle `None` by falling back to `Ordering::Equal` or `Ordering::Greater`.
```rust
dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
// OR
dists.sort_by(|a, b| a.total_cmp(b));
```