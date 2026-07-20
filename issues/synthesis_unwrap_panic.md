---
title: "Bug: Panic on partial_cmp in synthesis module"
labels: ["bug", "agent-found"]
---

### Description
In `mythrax-core/src/cognitive/synthesis.rs`, the code uses `.unwrap()` on the result of `f32::partial_cmp` during sorting of distances. `f32::partial_cmp` returns `None` if either value is `NaN`. If any embedding distance computation results in `NaN`, the sort operation will panic, crashing the daemon.

### File and Line Number
* `mythrax-core/src/cognitive/synthesis.rs`, line 631
* `mythrax-core/src/cognitive/synthesis.rs`, line 636

### Minimal Reproducible Scenario
1. Submit an invalid input or trigger a condition that results in a vector of zeros or all-same elements such that calculating cosine distance yields `0.0 / 0.0` (which is `NaN`).
2. The `cosine_distance` function computes the distance.
3. The resulting `NaN` value is appended to the `dists` vector.
4. `dists.sort_by(|a, b| a.partial_cmp(b).unwrap())` is called.
5. The daemon panics.

### Severity
**High** - Unhandled panics from runtime inputs crash the background intelligence daemon.

### Suggested Fix
Replace `.unwrap()` with `.unwrap_or(std::cmp::Ordering::Equal)` to safely handle `NaN` cases, or use a robust float wrapper (e.g., `ordered_float` or implement a safe comparison).
```rust
dists.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
```
