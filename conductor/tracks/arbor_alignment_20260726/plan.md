# Implementation Plan: Arbor Framework Alignment & Single-Pass Chunked Ingestion

## Phase 1: High-Speed Single-Pass Chunked Ingestion Engine

- [ ] Task: Update `crud_operations.rs` to execute SurrealDB batch insertions in 50-episode transactional chunks.
- [ ] Task: Batch IDF index updates into a single set-based SQL query.
- [ ] Task: Suppress `watcher.rs` filesystem syncing during bulk ingestion when `IS_INGESTING == true`.
- [ ] Task: Update `ingestion.rs` to scan transcript directories in a single pass and sort chronologically by JSONL timestamps.
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 2: Arbor 4-Field Node Struct & Verbatim Evidence Distillation

- [ ] Task: Expose 4-field tuple accessors ($h_n, r_n, \iota_n, \mu_n$) on `HypothesisNode`, `WikiNode`, and `Episode` structs.
- [ ] Task: Update `distillation.rs` system prompts to enforce the 4-field Arbor summary contract.
- [ ] Task: Extend `enforce_symbol_integrity` to protect raw error evidence ($r_n$) and symbol references ($\mu_n$) verbatim.
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 3: Upward Insight Propagation & Negative Constraint Ideation Gating

- [ ] Task: Refactor `compactor.rs` to perform parent-child graph edge traversal (`TreePropagate`).
- [ ] Task: Format and export `Negative Constraints` into root scope compaction notes.
- [ ] Task: Package policy collector into `TreeView(format="constraints")` for Cloud Brain dreaming and ideation.
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 4: Convergence Detection & Held-Out Validation Gates

- [ ] Task: Implement `ConvergenceDetector` with score velocity tracking and parent exhaustion signals in `arbor.rs`.
- [ ] Task: Refactor `evaluate_admission_gate` into formal `Etest` and `GitMergeBranch` wrappers for rule promotion.
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)
