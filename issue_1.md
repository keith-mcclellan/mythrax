---
title: "Bug: Float `NaN` Panic in Cosine Distance Sorting"
labels: ["bug", "agent-found"]
---

# Float `NaN` Panic in Cosine Distance Sorting

**File:** `mythrax-core/src/cognitive/synthesis.rs`
**Lines:** 497, 502

## Description
In `mythrax-core/src/cognitive/synthesis.rs`, dynamic epsilon calibration sorts distances calculated using `cosine_distance(u, v)`.
`cosine_distance` delegates to `crate::math::cosine_similarity(u, v)`. If an embedding vector contains all zeros, or values that result in a zero norm or float overflow leading to `Inf`/`NaN`, `cosine_similarity` will return `NaN`. (While `cosine_similarity` handles `norm == 0.0` explicitly, other invalid floats like `NaN` or `Inf` from LLM outputs could still propagate).
Because `1.0 - NaN` is `NaN`, the `dists` array will contain `NaN` elements. Calling `dists.sort_by(|a, b| a.partial_cmp(b).unwrap())` will then crash the process with a panic because `f32::partial_cmp` returns `None` when comparing `NaN`. This represents a severe denial of service or loop-crashing bug if an agent retrieves or processes a corrupted/zeroed embedding vector.

## Minimal Reproducible Scenario
1. A corrupted `wiki_node` or `episode` is saved into the database with an embedding containing extremely large numbers, or an embedding model produces `NaN`.
2. The background `compactor` or `synthesis` process awakens and collects all embeddings in the database (line 484).
3. The nested loop on line 493 iterates over a sample, executing `cosine_distance`.
4. `cosine_distance` yields `NaN` which is pushed into `dists`.
5. `dists.sort_by(|a, b| a.partial_cmp(b).unwrap())` triggers a panic on line 497, tearing down the process.

## Severity
**High**. This will repeatedly panic on background compaction loops, permanently stalling memory compaction and synthesis until the offending database record is manually deleted.

## Suggested Fix
Replace `.unwrap()` with a fallback ordering or filter out `NaN` distances beforehand.
```rust
dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
```
Or use a `f32` wrapper that implements `Ord`, such as the `ordered-float` crate.
