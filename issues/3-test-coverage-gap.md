---
title: Test coverage gap identified in public functions
labels: ["bug", "agent-found"]
---
## Bug Description
**Files and Missing Functions:**
- `run_workspace_audit` in `mythrax-core/src/verify.rs`
- `strip_nulls` in `mythrax-core/src/mcp_routes.rs`
- `format_episode_or_parent` in `mythrax-core/src/mcp_routes.rs`
- `stringify_record_id` in `mythrax-core/src/vault/operations.rs`
- `verify_and_repair` in `mythrax-core/src/vault/service.rs`
- `session_id_from_corpus_id` in `mythrax-core/src/bench/metrics.rs`
- `get_episode_db` in `mythrax-core/src/db/crud_operations.rs`
- `update_idf_index_for_episode` in `mythrax-core/src/db/crud_operations.rs`
- `update_idf_index_batch_db` in `mythrax-core/src/db/crud_operations.rs`
- `count_pending_cognitive_tasks` in `mythrax-core/src/db/cognitive_tasks.rs`
- `get_injected_tasks_older_than_ttl` in `mythrax-core/src/db/cognitive_tasks.rs`
- `delete_pipeline_state` in `mythrax-core/src/db/cognitive_tasks.rs`
- `delete_cognitive_task` in `mythrax-core/src/db/cognitive_tasks.rs`
- `get_completed_cognitive_tasks` in `mythrax-core/src/db/cognitive_tasks.rs`
- `get_layer_norm` in `mythrax-core/src/llm/mlx_weights.rs`

**Minimal Reproducible Scenario:** These public functions are exposed as part of the core library or API, but have no corresponding tests in the eval harness or unit test suite. If regressions are introduced to these functions, the test suite will silently pass, but production instances relying on them will fail.

**Severity:** High (Silent failure risk)

**Suggested Fix:** Implement integration and unit tests for these uncovered functions, particularly focusing on edge cases, panic paths, and malformed inputs.
