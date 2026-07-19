# Implementation Plan: v2.5 Memory Engine Bootstrap & Stabilization

## Phase 1: Test-Driven Framework & Core Architecture
- [ ] Task: Implement `test_bootstrap_e2e.rs` using mock environments to establish success criteria (TDD mandate).
- [ ] Task: Add `skip_llm` Mode to Ingestion Pipeline (`ingestion.rs`, `vault_handlers.rs`, `main.rs`).
- [ ] Task: Implement Per-Episode Distillation (`synthesis.rs`, `crud_operations.rs`) and add `summary` field to the `Episode` schema in SurrealDB.
- [ ] Task: Temporally-Anchor Graph Edges (`synthesis.rs`, `ingestion.rs`).
- [ ] Task: Wire HTR Pipeline Integration (Arbor) — implement `backpropagate_directions()`, `promote_insight_to_direction()`, contradiction conflict nodes, and `superseded_by` edges.
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md).

## Phase 2: Feedback Loop Hardening & Configuration
- [ ] Task: Add Positional Correction Detection to Bulk Ingestion (`ingestion.rs`).
- [ ] Task: Harden Live Session Feedback Loop (`precompact.rs`).
- [ ] Task: Fix Agent-Driven Wisdom Provenance (`write_handlers.rs`).
- [ ] Task: Increase Cognitive Task TTL for Bootstrap to 30 mins (`llm/mod.rs`, `distillation.rs`).
- [ ] Task: Fix Graduation Decay No-Op computing actual age (`graduation_pipeline.rs`).
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md).

## Phase 3: Tech Debt Cleanups & Empirical Testing
- [ ] Task: Delete redundant E2E tests (`test_distillation.rs`, `test_schema_upgrades.rs`, etc.).
- [ ] Task: Remove `socket2` dependency from `Cargo.toml`.
- [ ] Task: Cleanly remove dead code masked by `#[allow(dead_code)]`.
- [ ] Task: Align `SKILL.md` with MCP schemas.
- [ ] Task: Run `DEVELOPER_DIR` empirical runtime risk test.
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md).

## Phase 4: MCP-Driven Bootstrapping Orchestration
- [ ] Task: Execute Clean Reset of the environment.
- [ ] Task: Spawn Callback Resolver.
- [ ] Task: Execute Fast Bulk Ingest with `skip_llm`.
- [ ] Task: Trigger Async Distillation to complete the bootstrap process.
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md).
