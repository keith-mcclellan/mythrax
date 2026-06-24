# Test Plan: Phase 1 Foundations (v0.9.x)

This document defines the test plan for verifying the Phase 1 Foundations features of Mythrax. All tests will be implemented in Rust under `mythrax-core/tests/`.

---

## Unit Tests

*   **[ ] `test_resolve_active_scope`**
    *   *Goal*: Verify parent-traversal scope sensing.
    *   *Setup*: Create a temporary directory structure: `/tmp/test-scope-root/nested-1/nested-2/`. Place a `.git` folder inside `/tmp/test-scope-root/`.
    *   *Execution*: Call the scope resolver starting from `/tmp/test-scope-root/nested-1/nested-2/`.
    *   *Assertion*: Verify that it traverses up and resolves the scope as `"test-scope-root"`.
*   **[ ] `test_error_signature_regex`**
    *   *Goal*: Verify that compiler/framework error signatures are correctly matched via regex.
    *   *Execution*: Pass standard stderr outputs (Rust compile errors, TypeScript compile errors, 401 Unauthorized, RocksDB lock errors) into the regex parser.
    *   *Assertion*: Verify that the correct signatures (`"E0432"`, `"TS2322"`, `"401 Unauthorized"`, `"RocksDB lock"`) are extracted.

---

## Integration Tests

*   **[ ] `test_auto_scope_search`**
    *   *Goal*: Verify scope-partitioned searches.
    *   *Setup*: Seed SurrealDB with two episodes: one with `scope: "smwl"` and one with `scope: "other-project"`.
    *   *Execution*: Set the environment variable `MYTHRAX_WORKSPACE_ROOT` to `/path/to/smwl` and run `SurrealBackend::search` without passing an explicit scope.
    *   *Assertion*: Verify that only the `"smwl"` episode is returned, and the `"other-project"` episode is filtered out.
*   **[ ] `test_temporal_trajectory_linking`**
    *   *Goal*: Verify that successive episodes are linked sequentially in the database.
    *   *Execution*: Call `save_episode` twice in sequence, passing the same `session_id`.
    *   *Assertion*: Run a database query to verify that a `followed_by` relationship edge was created pointing from the first episode to the second episode.
*   **[ ] `test_deep_insight_traversal`**
    *   *Goal*: Verify that deep searches traverse the temporal graph.
    *   *Setup*: Seed three episodes linked sequentially (A $\rightarrow$ B $\rightarrow$ C).
    *   *Execution*: Call `search` with `deep_insight: true` and a query that matches episode B.
    *   *Assertion*: Verify that the returned search result for B contains A in the `prev_episodes` array and C in the `next_episodes` array.
*   **[ ] `test_diagnose_failure_retrieval`**
    *   *Goal*: Verify that past resolutions are matched and retrieved under the latency constraint.
    *   *Setup*: Seed a `WisdomRule` with target pattern `"TS2322"`.
    *   *Execution*: Call `diagnose_error_internal` passing a stderr containing a TypeScript type error. Measure the execution duration.
    *   *Assertion*: Verify that the correct wisdom rule is returned, and the execution time is strictly $<5\text{ms}$.

---

## Acceptance Tests

*   **[ ] `test_mcp_diagnose_failure_tool`**
    *   *Goal*: Verify the JSON-RPC interface for the new MCP diagnostic tool.
    *   *Execution*: Call the MCP tool `diagnose_failure` passing mock stderr and stdout.
    *   *Assertion*: Verify that the tool returns a JSON object containing the correct `causal_explanation` and `prescribed_remedy`.
*   **[ ] `test_htr_executor_interception`**
    *   *Goal*: Verify that Arbor HTR executor automatically intercepts and decorates test failures.
    *   *Setup*: Instantiate `ArborExecutor` and run a test command designed to fail.
    *   *Execution*: Call `ArborExecutor::execute` with the mock database backend.
    *   *Assertion*: Verify that the returned `logs` string has the Mythrax auto-diagnostic footnote appended to the end.
