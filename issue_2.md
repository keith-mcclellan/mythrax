---
title: "Bug: Potential NaN panic in memory synthesis compaction"
labels: ["bug", "agent-found"]
---

### Vulnerability / Bug Description
In `mythrax-core/src/cognitive/synthesis.rs`, when calculating distance matrices for k-distances to find elbow points, the code attempts to sort an array of `f32` floats using `.partial_cmp(b).unwrap()`. If the calculated distance returns `NaN` (e.g., due to an embedding being an empty array or calculation underflow), `.partial_cmp` evaluates to `None`, and `.unwrap()` panics the thread.

### File and Line Number
- `mythrax-core/src/cognitive/synthesis.rs`, lines 371 and 376.

### Minimal Reproducible Scenario
1. Have an anomalous episode or a memory with a malformed text chunk embedded as an all-zero vector (or extremely low magnitudes triggering division by zero).
2. The memory synthesis component pulls the batch of vectors and processes the 100-item distance matrix loop.
3. The float comparisons fail due to `NaN`s, and the background daemon panics.

### Severity
**Medium**. While generating `NaN` distances is rare, this occurs in the background daemon's paging and compaction logic which might block further intelligence routing operations if it reliably panics on specific inputs.

### Suggested Fix
The fix has been applied. It involves passing a fallback ordering: `.unwrap_or(std::cmp::Ordering::Equal)` which safely handles the `NaN` cases without panicking.
