---
title: "Bug: Panic caused by unwrap() during distance sort if NaN is encountered"
labels: ["bug", "agent-found"]
---

## Description
In `mythrax-core/src/cognitive/synthesis.rs` around lines 371 and 376, arrays of `f32` distance metrics are sorted using `partial_cmp().unwrap()`:

```rust
dists.sort_by(|a, b| a.partial_cmp(b).unwrap());
```

## Reproducible Scenario
1. An embedding model returns an all-zero vector (e.g. from a blank input, or a tokenizer issue).
2. The `cosine_distance` calculates the dot product (0) and divides by magnitude or uses it such that it results in a `NaN` (or `dot_product` calculates `0.0 * 0.0` but some other calculation down the line leads to `NaN`). Actually, standard cosine distance 1.0 - (dot_product) with an all-zero vector is just `1.0 - 0.0 = 1.0`. But if magnitudes were normalized inside the embedding generation and it yielded `NaN` vectors, `partial_cmp` between `NaN` values returns `None`.
3. Calling `.unwrap()` on `None` panics the thread and crashes the pipeline.

## Severity
**Medium** - Triggers on unexpected embedding outputs or math errors.

## Suggested Fix
Use a fallback when `partial_cmp` returns `None` (e.g., treating them as equal) or use the `f32::total_cmp` method which correctly handles `NaN`.

```rust
dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
// Or in newer Rust:
dists.sort_by(|a, b| a.total_cmp(b));
```
