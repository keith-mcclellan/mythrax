# Implementation Plan: v2.5 Memory Engine Bootstrap & Stabilization

## Phase 1: Test-Driven Framework & Core Architecture
- [x] Task: Implement `test_bootstrap_e2e.rs` using mock environments to establish success criteria (TDD mandate). (ba0e762)
- [ ] Task: Add `skip_llm` Mode to Ingestion Pipeline (`ingestion.rs`, `vault_handlers.rs`, `main.rs`).
- [ ] Task: Implement Per-Episode Distillation (`synthesis.rs`, `crud_operations.rs`) and add `summary` field to the `Episode` schema in SurrealDB.
- [ ] Task: Temporally-Anchor Graph Edges (`synthesis.rs`, `ingestion.rs`).
- [ ] Task: Wire HTR Pipeline Integration - `backpropagate_directions()` into post-dream hook.
- [ ] Task: Wire HTR Pipeline Integration - `promote_insight_to_direction()` after insight creation.
- [ ] Task: Wire HTR Pipeline Integration - Fix contradiction resolution to preserve evidence and create conflict nodes.
- [ ] Task: Wire HTR Pipeline Integration - Fix compactor to create `superseded_by` edges and expand temporal ranges.
- [ ] Task: Wire HTR Pipeline Integration - Fix graduation to create graph edges from insights to wisdom.
- [ ] Task: Wire HTR Pipeline Integration - Fix ingestion to create DB edges for linked artifacts.
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
- [ ] Task: Integrate dead code (`#[allow(dead_code)]`) into the pipeline properly rather than eliminating it.
- [ ] Task: Align `SKILL.md` with MCP schemas.
- [ ] Task: Run `DEVELOPER_DIR` empirical runtime risk test.
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md).

## Phase 4: MCP-Driven Bootstrapping Orchestration
- [ ] Task: Execute Clean Reset of the environment.
- [ ] Task: Spawn Callback Resolver.
- [ ] Task: Execute Fast Bulk Ingest with `skip_llm`.
- [ ] Task: Trigger Async Distillation to complete the bootstrap process.
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md).
