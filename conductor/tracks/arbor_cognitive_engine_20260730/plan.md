# Implementation Plan: Arbor-Aligned Cognitive Memory Engine Replacement (v1.1)

## Phase 1: Core Contracts & Arbor Triplet Schema Data Model
- [x] Task: Define `Fact`, `FactSource` (Episode, Document, Code, ForgedDocument, Skill), `IdeaNode`, `IdeaStatus`, `PipelineConfig` in `contracts.rs`
- [x] Task: Implement `ArborNode` trait on `Fact` ($h_n, \iota_n, r_n, \mu_n$)
- [x] Task: Update `Episode::causal_insight` in `contracts.rs:146` to `Option<serde_json::Value>` storing typed JSON arrays of extracted facts
- [x] Task: Write TDD unit tests for `Fact` serialization, `ArborNode` trait accessors, `Episode` JSON array persistence, and `PipelineConfig` defaults in `contracts.rs`
- [x] Task: Delete obsolete flat-string insight fields from legacy structs in `contracts.rs`
- [x] Task: Run Conductor Principal Engineer Review (conductor-review)
- [x] Task: Run Adversarial CTO Reviewer Subagent (Fix-Resubmit Loop until unconditional APPROVED)
- [x] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 2: Core Cognitive Pipeline & Prompts Module (`cognitive/pipeline.rs`, `prompts.rs`, `db.rs`)
- [x] Task: Write TDD unit tests in `prompts.rs` verifying JSON schema validation for all 9 callback prompts
- [x] Task: Implement prompt builders in `cognitive/prompts.rs` (Prompts 1-9: Episode, Doc, Code, Forge, Skill, Hypothesis, Refinement, Ancestor Merge Synthesis, Graduation)
- [x] Task: Implement SurrealDB CRUD operations for `Fact`, `IdeaNode`, `PipelineConfig`, and `RefinementLog` in `cognitive/db.rs`
- [x] Task: Write TDD unit tests for greedy cosine clustering `cluster_facts()` (verifying cosine $\ge 0.75$, min size 3, content-derived embeddings, zero centroid vector math)
- [x] Task: Implement `extract_facts()`, `extract_from_document()`, `extract_from_code()`, `forge_document()`, `forge_skill()` in `cognitive/pipeline.rs`
- [x] Task: Implement HTR lifecycle functions: `form_hypotheses()`, `refine_hypotheses()`, `merge_validated_nodes()`, `graduate()` in `cognitive/pipeline.rs`
- [x] Task: Implement evidence flattening ($r_n$, $\mu_n$) during `merge_validated_nodes()` and 0-degree orphaned node GC sweep ($\le 0.20$) in `refine_hypotheses()`
- [x] Task: Run Conductor Principal Engineer Review (conductor-review)
- [x] Task: Run Adversarial CTO Reviewer Subagent (Fix-Resubmit Loop until unconditional APPROVED)
- [x] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 3: Infrastructure Extensions, Vault Ingestion & Legacy Cleanup (`forge.rs`, `distillation.rs`, `ingestion.rs`, `arbor.rs`)
- [ ] Task: Write TDD unit tests for `forge_document()` dual-path vault writing (Path A raw reference chunks, Path B fact extraction) and `forge_skill()`
- [ ] Task: Refactor `cognitive/forge.rs` to replace monolithic `ingest_document()` with `pipeline::forge_document()`, retaining PDF text extraction, TOC parsing, and section chunking
- [ ] Task: Refactor `cognitive/harvest.rs` to replace batch harvester with `pipeline::forge_skill()`, writing raw skill pages to `/wiki/skills/` and extracting `FactSource::Skill` Arbor triplets
- [ ] Task: Refactor `vault/distillation.rs`, deleting legacy regex functions (`extract_wisdom_from_document`, `process_wisdom_block`, `extract_decisions`) and routing document processing through `pipeline::extract_from_document()`
- [ ] Task: Extend `sync_workspace_docs_to_vault` in `vault/ingestion.rs` to collect source code files (`.rs`, `.py`, `.ts`, `.go`), tag as `WorkspaceFileType::Code`, and queue `extract_from_code()`
- [ ] Task: Wire git worktree test admission gate (`HeldOutEvaluator`, `TestCommandEvaluator`) in `cognitive/arbor.rs` into `merge_validated_nodes()` for code-impacting hypotheses
- [ ] Task: Run Conductor Principal Engineer Review (conductor-review)
- [ ] Task: Run Adversarial CTO Reviewer Subagent (Fix-Resubmit Loop until unconditional APPROVED)
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 4: Hook Architecture & Route Handler Integration (`stop.rs`, `precompact.rs`, `watcher.rs`, `vault_handlers.rs`, `reflect.rs`, `manage_handlers.rs`)
- [ ] Task: Write TDD tests for stop hook background fact extraction trigger in `hooks/stop.rs`
- [ ] Task: Update `hooks/stop.rs` to queue `pipeline::extract_facts()` via the bounded `CognitiveTask` table upon saving mined episodes
- [ ] Task: Update `hooks/precompact.rs` to replace monolithic `run_llm_critic` with `Fact` contradiction extraction and immediate `refine_hypotheses()` pass
- [ ] Task: Update `vault/watcher.rs` to queue document/code extractions via bounded `CognitiveTask` table
- [ ] Task: Update `mcp_routes/vault_handlers.rs` to route `write` and vault-wide batch `reprocess_markdown` to `pipeline::extract_from_document()` via `CognitiveTask`
- [ ] Task: Update `hooks/reflect.rs` to inject pruned hypotheses ($\le 0.20$) as negative policy constraints ("Actions to Avoid") via `collect_policy_context()`
- [ ] Task: Wire MCP `manage` actions (`extract`, `extract_code`, `ingest_forge`, `ingest_skill`, `hypothesize`, `refine`, `merge`, `config`) in `mcp_routes/manage_handlers.rs`
- [ ] Task: Run Conductor Principal Engineer Review (conductor-review)
- [ ] Task: Run Adversarial CTO Reviewer Subagent (Fix-Resubmit Loop until unconditional APPROVED)
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 5: Legacy Code Deletion & End-to-End Verification
- [ ] Task: Delete `cognitive/synthesis.rs` (~3,838 lines)
- [ ] Task: Delete `cognitive/compactor.rs` (~2,088 lines)
- [ ] Task: Delete `cognitive/critic.rs` (~200 lines)
- [ ] Task: Delete `cognitive/meta_skill.rs` (~300 lines)
- [ ] Task: Run unit test suite: `MYTHRAX_TEST_MOCK=1 cargo nextest run -p mythrax-core domain_cognitive`
- [ ] Task: Run full regression test suite: `MYTHRAX_TEST_MOCK=1 cargo nextest run`
- [ ] Task: Run dev50 benchmark gate: `bash scripts/verify_dev50.sh`
- [ ] Task: Run Conductor Principal Engineer Review (conductor-review)
- [ ] Task: Run Adversarial CTO Reviewer Subagent (Fix-Resubmit Loop until unconditional APPROVED)
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)
