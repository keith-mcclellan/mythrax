# Implementation Plan: Arbor Framework Alignment & Single-Pass Chunked Ingestion

Incorporates all findings from three adversarial CTO review rounds.

---

## Phase 1: High-Speed Single-Pass Chunked Ingestion Engine

- [ ] **1.1** Update `save_episodes_batch_db` (`crud_operations.rs` L434) to chunk episodes into 50-item transactional sub-batches (`episodes.chunks(50)`).
- [ ] **1.2** Implement chunk failure strategy: skip failed chunks, log error with chunk index and episode titles, continue processing remaining chunks.
- [ ] **1.3** Batch IDF index updates into a single set-based SQL query (`UPDATE idf_index SET count = count + 1 WHERE token IN $tokens;`).
- [ ] **1.4** Add `IS_INGESTING` guard to `sync_file_to_db` and `sync_file_to_db_with_cache` in `watcher.rs` (L670): `if IS_INGESTING.load(SeqCst) { return Ok(()); }`.
- [ ] **1.5** Wire `skip_llm` parameter in `bulk_ingest_vault` (`ingestion.rs` L613) — remove dead assignment `let _ = skip_llm;` and propagate to title generation bypass.
- [ ] **1.6** Update `bulk_ingest_vault` to scan all transcript directories in a single pass, parse internal JSONL timestamps, sort chronologically.
- [ ] **1.7** Phase Verification: Run `mythrax ingest --source ... --harness antigravity` against 1,000+ episodes. Verify no RocksDB lock panics, no transaction timeouts, and idempotent re-run skips processed episodes.

---

## Phase 2: Arbor 4-Field Node Schema & Distillation Prompt Upgrade

- [ ] **2.1** Add explicit 4-field tuple accessor methods on `HypothesisNode` (`contracts.rs` L496): `h_n()`, `r_n()`, `iota_n()`, `mu_n()` returning `&str` / `Option<&str>`.
- [ ] **2.2** Add 4-field accessor trait on `WikiNode` and `Episode` structs mapping existing fields to the Arbor schema.
- [ ] **2.3** Replace the generic system prompt in `run_summarization_task` (`distillation.rs` L159) with the explicit 4-field Arbor contract:
  ```
  ### 🎯 Hypothesis & Intent (hn)
  ### 📊 Factual Result & Raw Evidence (rn)
  ### 🧠 Distilled Insight & Causal Lessons (ιn)
  ### 🔑 Artifact References & Key Symbols (µn)
  ```
- [ ] **2.4** Extend `enforce_symbol_integrity` (`distillation.rs` L111) to protect both the `rn` raw evidence block and `µn` symbol block verbatim.
- [ ] **2.5** Phase Verification: Trigger scope summarization via MCP. Inspect generated summaries in `~/mythrax-vault/episodes/` to confirm all 4 Arbor fields with verbatim tracebacks.

---

## Phase 3: Coordinator Tool Boundaries & MCP Tools

- [ ] **3.1** Create `src/mcp_routes/arbor_handlers.rs` with MCP tool handlers for `TreeAddNode`, `TreeUpdateNode`, `TreePrune`.
- [ ] **3.2** Implement `TreeView` MCP tool with 5 format modes: `compact`, `full`, `node`, `pending`, `constraints`.
- [ ] **3.3** Expose `GitMergeBranch` as an explicitly callable MCP tool. Refactor `HeldOutEvaluator` into formal `Etest` struct. Merge only if $S_{test} > S_{test}(M_{best})$.
- [ ] **3.4** Rewire `handle_pre_invocation_hook` (L1790 in `manage_handlers.rs`) from `collect_policy_context` to `TreeView(format="constraints")`.
- [ ] **3.5** Phase Verification: Verify all 6 tools are callable via MCP. Verify `handle_pre_invocation_hook` correctly retrieves constraints from `TreeView`.

---

## Phase 4: Dead Code Removal & Architectural Fixes

- [ ] **4.1** Delete `backpropagate_insights` (`arbor.rs` L458-514). Replace with `TreePropagate` trait invocation.
- [ ] **4.2** Delete `decide_admission` (`arbor.rs` L516-585). Replace with `GitMergeBranch` MCP tool.
- [ ] **4.3** Delete `collect_policy_context` (`manage_handlers.rs` L2196-2300). (Prerequisite: Phase 3.4 completed.)
- [ ] **4.4** Refactor `LlmCriticEvaluator` (`arbor.rs` L96-145): eliminate nested tokio runtime inside `std::thread::spawn`. Use native async with `tokio::task::spawn_blocking`.
- [ ] **4.5** Fix `dispatch_batch` (`arbor.rs` L588-601): acquire `METAL_GPU_SEMAPHORE` before GPU-bound evaluations, or restrict `buffer_unordered` to 1.
- [ ] **4.6** Phase Verification: `cargo build --release --features mlx` succeeds. All existing tests pass. No compilation warnings from removed functions.

---

## Phase 5: Upward Propagation, Negative Constraints & TreePropagate

- [ ] **5.1** Define `TreePropagate` trait in `arbor.rs` encapsulating leaf-to-root insight abstraction logic.
- [ ] **5.2** Refactor `compact_scope` in `compactor.rs` to traverse `relates_to` / `parent_id` graph edges and abstract child insights into parent nodes: $\iota_{\text{parent}} \leftarrow \text{Abstract}(\{\iota_{\text{children}}\})$.
- [ ] **5.3** Format and export `Negative Constraints` from pruned/failed nodes into root scope compaction notes for `TreeView(format="constraints")` consumption.
- [ ] **5.4** Phase Verification: Trigger compaction. Verify parent nodes contain abstracted child insights. Verify negative constraints appear in Cloud Brain pre-invocation hook output.

---

## Phase 6: FSM Lifecycle, Convergence Detection & Budget Tracking

- [ ] **6.1** Implement `ConvergenceDetector` struct in `arbor.rs`:
  - Sliding window of last 5 node scores.
  - Compute $\Delta \text{score} / \Delta \text{visits}$.
  - Escalating signals: `warn` (velocity < 0.1), `paradigm_shift` (velocity < 0.01 for 3+ consecutive windows), `stop` (velocity = 0 for 5 windows).
- [ ] **6.2** Implement `parent_exhaustion` detection: flag when all child executions fail to improve baseline score.
- [ ] **6.3** Wrap coordinator in explicit FSM enforcing `IDEATE → EXECUTE → EVALUATE → PRUNE/MERGE` state transitions.
- [ ] **6.4** Add configurable `max_depth` parameter (default: 2) and budget tracking (token/wall-clock/iteration caps).
- [ ] **6.5** Implement `SearchIdeaContext` background worker for related-work annotation without blocking IDEATE.
- [ ] **6.6** Phase Verification: Run a full Arbor loop. Verify FSM transitions are enforced. Verify convergence detection triggers paradigm_shift on stalled trees. Verify depth limit prevents unbounded expansion.

---

## Phase 7: Parallel Executor & Final Integration

- [ ] **7.1** Implement `RunSubagentParallel` for concurrent hypothesis execution on isolated worktrees, with `METAL_GPU_SEMAPHORE` coordination.
- [ ] **7.2** Full integration test: Ingest 1,000+ episodes → trigger dreaming → run Arbor HTR loop → verify 4-field summaries, Tree propagation, convergence detection, and held-out validation gate.
- [ ] **7.3** Run full test suite: `MYTHRAX_TEST_MOCK=1 cargo nextest run -p mythrax-core`.
- [ ] **7.4** Git commit and push.
