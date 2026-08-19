---
labels: bug, agent-found
---
# Bug: Test coverage gaps for public functions
**File:** mythrax-core/src/main.rs:384, mythrax-core/src/embeddings.rs:23 (and others)
**Scenario:** Numerous public functions such as `spawn_swap_monitor_thread` (in `main.rs`), `evict_global_embedder` (in `embeddings.rs`), `get_embedding_cache_path`, `flush_dirty_default_cache`, `save_stm_file`, `adapt_claude_code` in `mythrax-core/src/` lack any corresponding test coverage in the `mythrax-core/tests/` integration suite.
**Severity:** Medium
**Fix:** Write comprehensive unit and integration tests for these core logic components in the `evals` or `tests` directory to prevent regressions.