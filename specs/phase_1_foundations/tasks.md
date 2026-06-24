# Tasks: Phase 1 Foundations (v0.9.x)

This document breaks down the implementation of Phase 1 Foundations into sequential, verifiable tasks.

---

## T1: Define SurrealDB Schema Extensions
*   **Purpose**: Update database schema initialization with first-class relation tables for temporal links and wisdom versioning.
*   **Related Requirements**: AC-1.2, AC-1.3
*   **Related Tests**: `test_temporal_trajectory_linking`, `test_deep_insight_traversal`
*   **Inputs**: None
*   **Actions**:
    *   Modify `mythrax-core/src/db/schema.rs`.
    *   Add `DEFINE TABLE IF NOT EXISTS followed_by SCHEMAFULL TYPE RELATION IN episode OUT episode;`.
    *   Add `DEFINE TABLE IF NOT EXISTS superseded_by SCHEMAFULL TYPE RELATION IN wisdom OUT wisdom;`.
    *   Add fields `duration` and `created_at` to `followed_by`, and `reason` and `created_at` to `superseded_by`.
*   **Expected Output**: SurrealDB compiles and initializes the schema with the new tables on startup.
*   **Validation**: Start mythrax with an in-memory db, verify tables are registered.

---

## T2: Implement Active Scope Resolution
*   **Purpose**: Build the directory-traversal helper to resolve the active project scope name, including case-insensitive alphanumeric normalization.
*   **Related Requirements**: AC-1.1
*   **Related Tests**: `test_resolve_active_scope`
*   **Inputs**: Optional starting path
*   **Actions**:
    *   Add `resolve_active_scope` helper in `mythrax-core/src/db/backend.rs`.
    *   Read `MYTHRAX_WORKSPACE_ROOT` or `std::env::current_dir()`.
    *   Traverse up until finding `.git`, `.agents`, `Cargo.toml`, or `package.json`.
    *   Extract the leaf folder name, **normalize it to lowercase and strip all non-alphanumeric separators** (e.g. `mythrax-core` and `mythrax_core` both normalize to `"mythraxcore"`), and return as scope, or return `"general"` as fallback.
*   **Expected Output**: Correct normalized leaf directory name is returned based on folder structures.
*   **Validation**: Run `test_resolve_active_scope` test suite.

---

## T3: Integrate Auto-Scope Filtering in Search
*   **Purpose**: Auto-inject scope constraints into vector and wisdom queries when scope is not explicitly provided, using `$target_scope` and `$active_scope` to prevent database collisions.
*   **Related Requirements**: AC-1.1
*   **Related Tests**: `test_auto_scope_search`
*   **Inputs**: Search queries
*   **Actions**:
    *   Modify `SurrealBackend::search` and `SurrealBackend::get_wisdom` in `mythrax-core/src/db/backend.rs`.
    *   If `scope` is `None`, call `resolve_active_scope` to determine active scope.
    *   Modify queries to filter by `scope IN [$active_scope, "general"]`. Ensure never binding parameters as `$scope` to prevent SurrealDB variable collisions.
*   **Expected Output**: Search results filter out sibling project memories.
*   **Validation**: Run `test_auto_scope_search` integration test.

---

## T4: Implement Temporal Session Linking
*   **Purpose**: Track and link consecutive episodes sequentially inside the `save_episode` pipeline with task-level isolation.
*   **Related Requirements**: AC-1.2
*   **Related Tests**: `test_temporal_trajectory_linking`
*   **Inputs**: `EpisodeSave` with optional `session_id` and `task_id`
*   **Actions**:
    *   Update `save_episode` method signature in `StorageBackend` and `SurrealBackend` to accept `session_id` and `task_id`.
    *   In `save_episode`, if `session_id` is present, determine the tracking key: `_last_episode_id_<task_id>` if `task_id` is provided, falling back to `_last_episode_id` otherwise.
    *   Read the tracking key from the `short_term_memory` table.
    *   If found, run a SurrealQL query: `RELATE $last_id->followed_by->$new_id;`.
    *   Upsert the new episode ID into `short_term_memory` under the tracking key for that session.
*   **Expected Output**: Database creates `followed_by` edge records between sequential saves.
*   **Validation**: Run `test_temporal_trajectory_linking` integration test.

---

## T5: Implement Chronological Deep-Insight Search
*   **Purpose**: Retrieve adjacent chronological steps during deep search.
*   **Related Requirements**: AC-1.3
*   **Related Tests**: `test_deep_insight_traversal`
*   **Inputs**: Vector query with `deep_insight: true`
*   **Actions**:
    *   Modify search queries in `SurrealBackend::search` when `deep_insight` is true.
    *   Traverse `<-followed_by<-episode` and `->followed_by->episode` to retrieve adjacent nodes.
    *   Hydrate adjacent node details and append them to the `related_nodes` array of the search result.
*   **Expected Output**: Deep searches return chronological chains.
*   **Validation**: Run `test_deep_insight_traversal` integration test.

---

## T6: Implement Local Error Signature Matcher
*   **Purpose**: Build the high-speed CPU diagnostic retriever to resolve error remedies in $<5\text{ms}$ with HNSW vector search fallback.
*   **Related Requirements**: AC-1.4
*   **Related Tests**: `test_error_signature_regex`, `test_diagnose_failure_retrieval`
*   **Inputs**: `stdout`, `stderr`
*   **Actions**:
    *   Implement `diagnose_error_internal(&self, stderr: &str, stdout: &str) -> Result<Option<(String, String)>>` in `SurrealBackend`.
    *   Compile and run regex signatures for common compilers (Rust, TS/Node, DB lock patterns).
    *   If no regex matches, **run a fast local HNSW vector search on the raw error message itself, but enforce a high similarity threshold (0.70)**.
    *   Return `(causal_explanation, prescribed_remedy)`.
*   **Expected Output**: Signature is matched and remedy retrieved in under 5 milliseconds.
*   **Validation**: Run `test_error_signature_regex` and `test_diagnose_failure_retrieval` test suites.

---

## T7: Integrate Diagnostics in HTR Executor
*   **Purpose**: Intercept test command failures in Arbor HTR and append the diagnostic remedy to returned logs, passing the database backend reference as a parameter.
*   **Related Requirements**: AC-1.5
*   **Related Tests**: `test_htr_executor_interception`
*   **Inputs**: Executor test results
*   **Actions**:
    *   Update `ArborExecutor::execute` to receive a reference to `SurrealBackend` (or dyn StorageBackend) as a parameter, keeping the executor struct stateless.
    *   If `status.success()` is false, call `backend.diagnose_error_internal(&stderr, &stdout)`.
    *   If a remedy is found, format it as a markdown warning footnote and append it directly to `combined_logs` returned to the coordinator.
*   **Expected Output**: Fails in HTR automatically contain diagnostic remedies.
*   **Validation**: Run `test_htr_executor_interception` acceptance test.

---

## T8: Expose MCP `diagnose_failure` Tool
*   **Purpose**: Expose the JSON-RPC tool interface in the MCP server to support manual agent workflow diagnostics.
*   **Related Requirements**: AC-1.4
*   **Related Tests**: `test_mcp_diagnose_failure_tool`
*   **Inputs**: JSON-RPC request
*   **Actions**:
    *   Modify `mythrax-core/src/mcp.rs`.
    *   Add `diagnose_failure` to `tools/list` schema.
    *   In `handle_request`, map `"diagnose_failure"` to `backend.diagnose_error_internal`.
    *   Format and return the output as a standard MCP text response.
*   **Expected Output**: JSON-RPC client can query the tool and receive structured remedies.
*   **Validation**: Run `test_mcp_diagnose_failure_tool` acceptance test.

---

## T9: Write and Execute Core Test Suite
*   **Purpose**: Consolidate and execute the full test suite to guarantee zero regressions.
*   **Related Requirements**: All ACs
*   **Related Tests**: All tests
*   **Inputs**: Source code
*   **Actions**:
    *   Create or update test files under `mythrax-core/tests/`.
    *   Run `cargo test` and verify that all unit, integration, and acceptance tests compile and pass successfully.
*   **Expected Output**: Cargo test suite passes cleanly.
*   **Validation**: Verify console output: `test result: ok`.
