# Implementation Plan: Arbor Framework Alignment & Single-Pass Chunked Ingestion

Incorporates all findings from 3 adversarial CTO reviews + 1 forensic root-cause investigation.

---

> [!CAUTION]
> **Why This Has Failed 3 Times (Forensic Root Cause)**:
> The project treated Arbor as a prompt-engineering pattern instead of a structural data contract. Previous attempts mapped the 4-field node tuple onto flat `content: String` columns, asked the LLM to output structured markdown, and stored the result monolithically. This creates an impedance mismatch — the system looks like Arbor at the surface but acts like a flat RAG system underneath. The fix requires structural schema changes, not just prompt changes.

> [!WARNING]
> **Phase Ordering is Load-Bearing**: The forensic CTO identified that the previous plan would delete `backpropagate_insights` (Phase 4) before building its replacement `TreePropagate` (Phase 5), breaking the system during testing. This revised plan fixes the ordering: build replacements FIRST, then remove dead code.

> [!WARNING]
> **Silent Tech Debt**: `grep -rn "TODO|FIXME|HACK|STUB"` returns 0 hits across the entire `src/` tree. All structural flaws appear fully implemented on the surface but are architecturally broken underneath. There are no warning markers.

---

## Phase 0: Immediate Safety Clamps (Pre-Requisite)

These must be applied BEFORE any other work to prevent crashes during testing.

- [ ] **0.1** Add `IS_INGESTING` guard at top of `sync_file_to_db` and `sync_file_to_db_with_cache` in `watcher.rs` (L670): `if IS_INGESTING.load(SeqCst) { return Ok(()); }`.
- [ ] **0.2** Restrict `dispatch_batch` (`arbor.rs` L588) from `buffer_unordered(2)` → `buffer_unordered(1)` to prevent GPU OOM during all subsequent test runs.
- [ ] **0.3** Wire `skip_llm` parameter in `bulk_ingest_vault` (`ingestion.rs` L613) — remove dead assignment `let _ = skip_llm;`.
- [ ] **0.4** Verify: `cargo build --release --features mlx`. Run ingestion dry-run — no RocksDB panics, no GPU OOM.

---

## Phase 1: Chunked Ingestion Engine

- [ ] **1.1** Chunk `save_episodes_batch_db` (`crud_operations.rs` L434) into 50-item transactional sub-batches.
- [ ] **1.2** Implement skip-and-continue failure strategy: skip failed chunks, log error with chunk index and episode titles, continue remaining chunks. Pipeline is idempotent — re-runs skip processed episodes via `existing_titles` dedup.
- [ ] **1.3** Batch IDF index updates into a single set-based SQL query.
- [ ] **1.4** Update `bulk_ingest_vault` for single-pass scan with JSONL timestamp sorting.
- [ ] **1.5** Verify: Ingest 1,000+ episodes. Confirm no transaction timeouts, idempotent re-run.

---

## Phase 2: 4-Field Schema Normalization (The Root Cause Fix)

This is the structural change that previous versions skipped. `HypothesisNode` already has the right fields. `Episode` and `WikiNode` do not.

- [ ] **2.1** Add structured Arbor fields to `Episode` (`contracts.rs` L98):
  - `hypothesis: Option<String>` — maps to $h_n$. (Note: `outcome` at L154 is a partial analog but semantically different — `outcome` records the result state, not the hypothesis intent.)
  - `raw_evidence: Option<String>` — maps to $r_n$. Holds verbatim tracebacks/diffs/test output. Must NOT be included in the embedding vector.
  - `causal_insight: Option<String>` — maps to $\iota_n$. (Note: `causal_explanation` at L156 exists but is never populated by the distillation pipeline.)
  - `artifact_refs: Option<Vec<String>>` — maps to $\mu_n$. Verbatim file paths, function signatures, git refs.
- [ ] **2.2** Add structured Arbor fields to `WikiNode` (`contracts.rs` L551):
  - `hypothesis: Option<String>`, `raw_evidence: Option<String>`, `causal_insight: Option<String>`, `artifact_refs: Option<Vec<String>>`.
- [ ] **2.3** Update SurrealQL schema definitions in `schema.rs` to register the new columns.
- [ ] **2.4** Add 4-field accessor trait `ArborNode` with methods `h_n()`, `r_n()`, `iota_n()`, `mu_n()`. Implement for `HypothesisNode`, `Episode`, `WikiNode`.
- [ ] **2.5** Update `run_summarization_task` (`distillation.rs` L154-209):
  - Replace generic `"You are a code summarizer"` system prompt with the 4-field Arbor structural contract.
  - Parse the LLM response into the 4 discrete struct fields instead of dumping into `content`.
  - Copy raw tracebacks verbatim into `raw_evidence` — do NOT summarize them.
- [ ] **2.6** Extend `enforce_symbol_integrity` (`distillation.rs` L111) to protect both `raw_evidence` and `artifact_refs` blocks.
- [ ] **2.7** Update embedding generation to use `causal_insight` (not `content` or `raw_evidence`) as the embedding source text, preventing traceback noise from polluting vector search.
- [ ] **2.8** Verify: Trigger scope summarization. Inspect summaries — confirm 4 separate fields populated, raw tracebacks verbatim, embeddings based on `causal_insight` only.

---

## Phase 3: Coordinator MCP Tool Boundaries

- [ ] **3.1** Create `src/mcp_routes/arbor_handlers.rs` with MCP handlers for `TreeAddNode`, `TreeUpdateNode`, `TreePrune`. These write to the normalized 4-field schema from Phase 2.
- [ ] **3.2** Implement `TreeView` MCP tool with 5 format modes: `compact`, `full`, `node`, `pending`, `constraints`.
- [ ] **3.3** Expose `GitMergeBranch` as explicitly callable MCP tool. Refactor `HeldOutEvaluator` into formal `Etest` struct. Merge only if $S_{test} > S_{test}(M_{best})$.
- [ ] **3.4** Rewire `handle_pre_invocation_hook` (L1790 in `manage_handlers.rs`) from `collect_policy_context` to `TreeView(format="constraints")`.
- [ ] **3.5** Verify: All 6 MCP tools callable. `handle_pre_invocation_hook` retrieves constraints from `TreeView`.

---

## Phase 4: TreePropagate & Negative Constraints (BUILD REPLACEMENTS FIRST)

This phase MUST complete before Phase 5 (dead code removal).

- [ ] **4.1** Define `TreePropagate` trait in `arbor.rs` encapsulating leaf-to-root insight abstraction.
- [ ] **4.2** Implement `TreePropagate` for `HypothesisNode`, replacing the logic in `backpropagate_insights`.
- [ ] **4.3** Update `compact_scope` in `compactor.rs`: retain DBSCAN for episode clustering, but add explicit negative constraint extraction from clustered episodes and propagate them to parent scope nodes.
- [ ] **4.4** Format and export negative constraints to root scope compaction notes for `TreeView(format="constraints")`.
- [ ] **4.5** Verify: Trigger compaction. Parent nodes contain abstracted child insights. Negative constraints appear in pre-invocation hook output.

---

## Phase 5: Dead Code Removal & Architectural Fixes

Prerequisites: Phase 3 (tools built), Phase 4 (replacements built).

- [ ] **5.1** Delete `backpropagate_insights` (`arbor.rs` L458-514). Replaced by `TreePropagate` from Phase 4.
- [ ] **5.2** Delete `decide_admission` (`arbor.rs` L516-585). Replaced by `GitMergeBranch` from Phase 3.
- [ ] **5.3** Delete `collect_policy_context` (`manage_handlers.rs` L2196-2300). Replaced by `TreeView` from Phase 3.
- [ ] **5.4** Refactor `LlmCriticEvaluator` (`arbor.rs` L96-145): eliminate nested tokio runtime inside `std::thread::spawn`, use native async with `tokio::task::spawn_blocking`.
- [ ] **5.5** Verify: `cargo build --release --features mlx`. All tests pass. No warnings from removed functions.

---

## Phase 6: FSM Lifecycle, Convergence Detection & Budget Tracking

- [ ] **6.1** Implement `ConvergenceDetector` struct in `arbor.rs`:
  - Sliding window of last 5 node scores.
  - Score velocity: $\Delta S / \Delta V$.
  - Escalating signals: `warn` (velocity < 0.1), `paradigm_shift` (velocity < 0.01 for 3+ windows), `stop` (velocity = 0 for 5 windows).
- [ ] **6.2** Implement `parent_exhaustion` detection.
- [ ] **6.3** Wrap coordinator in explicit FSM: `IDEATE → EXECUTE → EVALUATE → PRUNE/MERGE`.
- [ ] **6.4** Add `max_depth` (default: 2) and budget tracking (token/wall-clock/iteration).
- [ ] **6.5** Implement `SearchIdeaContext` background worker (DEFERRED — optimization, not load-bearing).
- [ ] **6.6** Verify: Run Arbor loop. FSM enforced. Convergence triggers paradigm_shift on stalled trees.

---

## Phase 7: Integration Test & Ship

- [ ] **7.1** Restore `dispatch_batch` to `buffer_unordered(2)` WITH proper `METAL_GPU_SEMAPHORE` acquisition in executor (or keep at 1 if GPU memory insufficient).
- [ ] **7.2** Full integration: Ingest 1,000+ episodes → dreaming → Arbor HTR loop → verify 4-field summaries, tree propagation, convergence, held-out gate.
- [ ] **7.3** Run full test suite: `MYTHRAX_TEST_MOCK=1 cargo nextest run -p mythrax-core`.
- [ ] **7.4** Git commit and push.
