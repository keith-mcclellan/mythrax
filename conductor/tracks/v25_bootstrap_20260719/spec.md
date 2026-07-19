# Specification: v2.5 Memory Engine Bootstrap & Stabilization

## Overview
Bootstrap Mythrax in a workspace with 1000+ historical transcripts using a two-phase architecture: fast ingestion with local embeddings followed by async LLM distillation (episode titles, summaries, insights, wisdom) via cognitive callbacks. Hardens the feedback loops across all entry points, cleans up test suite redundancy, enforces temporally-anchored graph edges, resolves Arbor HTR structural gaps, and actually orchestrates the bulk bootstrap ingestion.

## Functional Requirements
1. **Test-Driven E2E Protocol**: `test_bootstrap_e2e.rs` must be implemented *first* to define exact success criteria for the `episode -> insight -> direction -> wisdom` pipeline.
2. **skip_llm Ingestion Mode**: Add `skip_llm` parameter to `bulk_ingest_vault` to skip LLM title/summary generation but keep fast local MLX embeddings and dependency graph generation.
3. **Async Distillation**: Implement `distill_episode_metadata()` during dreaming to generate episode titles and summaries via cloud callbacks. Must explicitly add `summary` field to the `Episode` schema in SurrealDB.
4. **Temporally-Anchored Graph Edges**: Update `synthesis.rs` and `ingestion.rs` so that derived nodes inherit the original episode timestamps to ensure time decay (`valid_from`/`valid_to`) functions properly.
5. **HTR Pipeline Integration (Arbor)**: Wire `backpropagate_directions()`, `promote_insight_to_direction()`, fix contradiction resolution to create conflict nodes, and fix `compactor.rs` to create `superseded_by` edges.
6. **Feedback Loop Hardening**: 
   - Add positional correction tracking to bulk ingestion.
   - Trigger LLM Critic and semantic edges in live sessions.
   - Fix wisdom provenance in agent-driven MCP endpoints.
7. **Test Consolidation**: Delete overly permissive/redundant tests.
8. **Documentation Alignment**: Synchronize `SKILL.md` with MCP schemas.

## Non-Functional Requirements
- Maintain ingestion latency (<15ms per episode for embeddings).
- Ensure VRAM safety during dreaming (no concurrent Metal GPU context loading).
- Backwards compatibility with existing Mythrax 2.4 datasets.

## Acceptance Criteria
- `test_bootstrap_e2e.rs` passes (with all 23 structural/temporal assertions), proving full pipeline correctness.
- 1000+ transcripts can be successfully ingested and asynchronously distilled.
- `DEVELOPER_DIR` empirical test is run and documented.
- No `#[allow(dead_code)]` or redundant test files remain.
