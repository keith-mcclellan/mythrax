# Implementation Plan: v2.5 Memory Engine Bootstrap & Stabilization

## Phase 1: Test-Driven Framework & Core Architecture [checkpoint: 949cf28]
- [x] Task: Implement `test_bootstrap_e2e.rs` using mock environments to establish success criteria (TDD mandate). (ba0e762)
- [x] Task: Add `skip_llm` Mode to Ingestion Pipeline (`ingestion.rs`, `vault_handlers.rs`, `main.rs`). (0b310f8)
- [x] Task: Implement Per-Episode Distillation (`synthesis.rs`, `crud_operations.rs`) and add `summary` field to the `Episode` schema in SurrealDB. (f901aac)
- [x] Task: Temporally-Anchor Graph Edges (`synthesis.rs`, `ingestion.rs`). (f0b441e)
- [x] Task: Wire HTR Pipeline Integration - `backpropagate_directions()` into post-dream hook. (0fc6f6b)
- [x] Task: Wire HTR Pipeline Integration - `promote_insight_to_direction()` after insight creation. (30f3bd0)
- [x] Task: Wire HTR Pipeline Integration - Fix contradiction resolution to preserve evidence and create conflict nodes. (5f37275)
- [x] Task: Wire HTR Pipeline Integration - Fix compactor to create `superseded_by` edges and expand temporal ranges. (bd0b205)
- [x] Task: Wire HTR Pipeline Integration - Fix graduation to create graph edges from insights to wisdom. (60fe3ae)
- [x] Task: Wire HTR Pipeline Integration - Fix ingestion to create DB edges for linked artifacts. (f60d3a5)
- [x] Task: Phase Verification & Checkpoint (Refer to workflow.md). (949cf28)

## Phase 2: Feedback Loop Hardening & Configuration
- [x] Task: Add Positional Correction Detection to Bulk Ingestion (`ingestion.rs`). (4214a41)
- [x] Task: Harden Live Session Feedback Loop (`precompact.rs`). (41d20ed)
- [x] Task: Fix Agent-Driven Wisdom Provenance (`write_handlers.rs`). (41d20ed)
- [x] Task: Increase Cognitive Task TTL for Bootstrap to 30 mins (`llm/mod.rs`, `distillation.rs`). (2869574)
- [x] Task: Fix Graduation Decay No-Op computing actual age (`graduation_pipeline.rs`). (94854c5)
- [x] Task: Phase Verification & Checkpoint (Refer to workflow.md). (94854c5)

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

## Phase: Review Fixes
- [x] Task: Apply review suggestions (949cf28)
