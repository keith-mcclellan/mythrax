---
title: "Bug: Missing test coverage for PagingManager context window management"
labels: ["bug", "agent-found"]
---

### Description
In `mythrax-core/src/cognitive/memory_os.rs`, `PagingManager::evict_if_needed` (and `PagingManager::access_node`) has no unit test or evaluation harness coverage. Given this code orchestrates context windows for AI agents (which is highly susceptible to off-by-one and logical errors), this lack of coverage violates the directive to test all core public methods.

### Minimal Reproducible Scenario
Run `cargo test` in `mythrax-core/` and search for any test invoking `PagingManager::evict_if_needed`. None exist.

### Severity
Medium (Test Gap).

### Suggested Fix
Add a `#[cfg(test)]` module inside `mythrax-core/src/cognitive/memory_os.rs` that tests eviction ordering, pinned node preservation, and exact capacity boundaries.
