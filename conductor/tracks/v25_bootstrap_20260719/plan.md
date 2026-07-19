# Implementation Plan: v2.5 Memory Engine Bootstrap & Stabilization

## Phase 1: Ingestion & Distillation Architecture
- [ ] Task: Add `skip_llm` Mode to Ingestion Pipeline (`ingestion.rs`, `vault_handlers.rs`, `main.rs`)
- [ ] Task: Implement Per-Episode Distillation (`synthesis.rs`, `crud_operations.rs`)
- [ ] Task: Harden Live Session Feedback Loop (`precompact.rs`)
- [ ] Task: Fix Agent-Driven Wisdom Provenance (`write_handlers.rs`)
- [ ] Task: Add Positional Correction Detection to Bulk Ingestion (`ingestion.rs`)
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 2: Configuration & Graduation Fixes
- [ ] Task: Increase Cognitive Task TTL for Bootstrap to 30 mins (`llm/mod.rs`, `distillation.rs`)
- [ ] Task: Fix Graduation Decay No-Op computing actual age (`graduation_pipeline.rs`)
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 3: Test Consolidation & Tech Debt Cleanups
- [ ] Task: Delete redundant E2E tests (`test_distillation.rs`, `test_schema_upgrades.rs`, etc.)
- [ ] Task: Remove `socket2` dependency from `Cargo.toml`
- [ ] Task: Cleanly remove dead code masked by `#[allow(dead_code)]`
- [ ] Task: Align `SKILL.md` with MCP schemas
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 4: E2E Verification
- [ ] Task: Implement `test_bootstrap_e2e.rs` using mock environments
- [ ] Task: Run full regression test suite (280+ tests)
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)
