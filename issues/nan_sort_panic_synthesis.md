---
labels: bug, agent-found, architecture-review
title: "CTO Review: NaN Sorting Panic in Epsilon Calibration"
---

## Bug Description
In `mythrax-core/src/cognitive/synthesis.rs`, vectors of `f32` cosine distances are sorted using `partial_cmp().unwrap()`. In Rust, `f32::partial_cmp` returns `None` if either value is `NaN`. If any distance calculation results in `NaN` (which can happen with zero-length embedding vectors due to division by zero during cosine distance calculation), the `unwrap()` will panic, crashing the synthesis loop.

## File and Line Number
- File: `mythrax-core/src/cognitive/synthesis.rs`
- Line: 631 and 636 (before fix)

## Reproducible Scenario
1. Provide an episode or wiki node with an empty or zero-magnitude embedding vector (e.g., `[0.0; 1536]`).
2. Run the dynamic epsilon calibration in the synthesis loop, which enters the branch `if embeddings.len() >= 100 { ... }`.
3. The cosine distance calculation between the zero-magnitude vector and any other vector divides by zero, yielding `NaN`.
4. The `dists.sort_by(|a, b| a.partial_cmp(b).unwrap())` encounters the `NaN` value.
5. The `partial_cmp` returns `None`, and `unwrap()` panics, bringing down the entire asynchronous cognitive synthesis task.

## Severity
**High**. A single malformed embedding from an external ML model or database corruption can consistently crash the core synthesis process, creating a denial of service for memory compaction.

## Suggested Fix
Replace `.unwrap()` with `.unwrap_or(std::cmp::Ordering::Equal)` when sorting floating-point distances to safely handle `NaN` values without panicking.
```rust
dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
```
