# Bug: Test coverage gap in pipeline signals public functions

**File and Line Number**: `mythrax-core/src/cognitive/pipeline/signals.rs`, lines 6, 27, 34, 41, 118

**Minimal Reproducible Scenario**:
The functions `slugify_title`, `derive_slug`, `resolve_rule_path`, `cluster_facts`, and `get_active_stm_anchors` are public and critical for agent memory organization, yet a codebase grep reveals zero corresponding tests in `evals/` or the file itself. Refactoring these functions risks silent regressions that won't be caught by CI.

**Severity**: Medium

**Suggested Fix**:
Add a `#[cfg(test)] mod tests { ... }` block to `mythrax-core/src/cognitive/pipeline/signals.rs` (or equivalent integration tests in `evals/`) covering positive, negative, and edge cases for these functions.

**Labels**: `bug`, `agent-found`