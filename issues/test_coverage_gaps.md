---
title: "Bug: Test Coverage Gaps in mythrax-core"
labels: bug, agent-found
---
## Issue
Files: `mythrax-core/src/llm/mlx_weights.rs` (line 60), `mythrax-core/src/hooks/stop.rs` (line 9), `mythrax-core/src/hooks/shell.rs` (lines 54, 66, 73), `mythrax-core/src/hooks/adapters.rs` (lines 36, 45, 49, 53, 57, 71), `mythrax-core/src/retrieval/boosts.rs` (line 31), `mythrax-core/src/cognitive/paging.rs` (line 72).

## Minimal Reproducible Scenario
These files expose core public functions (e.g., `get_layer_norm`, `sanitize_session_id`, `apply_boosts`) that lack any corresponding unit or integration test coverage. Refactoring these functions can result in silent regressions without test validation.

## Severity
Medium

## Suggested Fix
Add `#[cfg(test)] mod tests { ... }` blocks with comprehensive unit tests for each file to ensure edge-cases are validated.
