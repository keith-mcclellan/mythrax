---
title: "AI Embedding NaN Panic Path in Distance Sort"
labels: ["bug", "agent-found"]
---

## Description
A sorting logic panic path exists in the memory synthesis compaction routines. The system sorts distances (calculated using `cosine_distance` over vectors) by unwrapping `partial_cmp`. If the underlying vector API or calculations yield `NaN` values, `partial_cmp` returns `None`, and `.unwrap()` panics.

## Location
`mythrax-core/src/cognitive/synthesis.rs`, lines 371 & 376

## Minimal Reproducible Scenario
When the `mythrax` engine attempts to perform structural memory clustering, it queries an embedding endpoint. If that endpoint behaves erratically and returns a vector of zeros, or if network failure leads to an unhandled empty embedding block, `cosine_distance` can result in a divide-by-zero causing a floating-point `NaN`.

During elbow point detection:
```rust
dists.sort_by(|a, b| a.partial_cmp(b).unwrap());
```
If `a` or `b` is `NaN`, `unwrap()` triggers a panic, bringing down the whole synthesis thread.

## Severity
High (false positive success resulting in a catastrophic panic)

## Suggested Fix
Gracefully handle `None` from `partial_cmp`, sorting NaNs deterministically or stripping them before sorting:

```rust
dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
```
Or preferably, filter out `NaN` values from `dists` before the sort operation.
