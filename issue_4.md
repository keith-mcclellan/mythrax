---
title: "Bug: Critical Test Coverage Gaps in Cognitive Modules"
labels: ["bug", "agent-found"]
severity: "Medium"
---

## Description
Several core modules within `mythrax-core/src/cognitive/` contain zero unit or integration tests, despite exposing complex, state-mutating public APIs. This makes the system extremely brittle to refactoring and violates architectural guidelines for safety and correctness.

## File and Line Numbers
- `mythrax-core/src/cognitive/compactor.rs` (0 tests)
- `mythrax-core/src/cognitive/meta_skill.rs` (0 tests)
- `mythrax-core/src/cognitive/memory_os.rs` (0 tests)
- `mythrax-core/src/cognitive/arbor.rs` (0 tests)
- `mythrax-core/src/cognitive/executor.rs` (0 tests)
- `mythrax-core/src/cognitive/paging.rs` (0 tests)

## Minimal Reproducible Scenario
1. Run `cargo test` in `mythrax-core`.
2. Observe that no tests are executed for the aforementioned files.
3. Introduce a logical error (e.g., reversing an inequality in the compactor threshold check).
4. Run tests again; the test suite passes, silently allowing a critical logic bug into production.

## Blast Radius
Silent regressions in the cognitive architecture (memory eviction, skill synthesis, action execution) can persist undetected, leading to agent degradation or catastrophic failure loops.

## Suggested Fix
Implement minimal unit tests for the core public functions in these modules. For example:
- `memory_os.rs`: Test `evict_if_needed` against LRU logic.
- `compactor.rs`: Test `cosine_similarity` logic and `compact_global` deduplication boundaries.
- `paging.rs`: Test `extract_symbols` with standard Rust, TS, and Py file payloads.