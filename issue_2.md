# Finding: Daemon Panic on NaN embeddings

**Finding:** `mythrax-core/src/cognitive/synthesis.rs` iterates over distance distributions and uses `.partial_cmp(b).unwrap()` without checking for NaN.

**Current Assumption:** Embedded representation comparisons always produce standard numeric outputs.

**Attack Scenario:** A crafted document injection results in `NaN` when interacting with the MLX backend.

**Blast Radius:** The daemon panics during the memory synthesis phase, halting compaction tasks.

**Recommended Structural Change:** Fallback to `.unwrap_or(std::cmp::Ordering::Equal)` or exclude NaN elements explicitly during the sorting phase.

Labels: `bug`, `agent-found`, `architecture-review`, `adversarial`
