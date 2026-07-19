# Specification: v2.5 Memory Engine Bootstrap & Stabilization

## Overview
Bootstrap Mythrax in a workspace with 1000+ historical transcripts using a two-phase architecture: fast ingestion with local embeddings followed by async LLM distillation (episode titles, summaries, insights, wisdom) via cognitive callbacks. Hardens the feedback loops across all entry points, cleans up test suite redundancy, and aligns documentation.

## Functional Requirements
1. **skip_llm Ingestion Mode**: Add `skip_llm` parameter to `bulk_ingest_vault` to skip LLM title/summary generation but keep fast local MLX embeddings and dependency graph generation.
2. **Async Distillation**: Implement `distill_episode_metadata()` during dreaming to generate episode titles and summaries via cloud callbacks.
3. **Graph Validation**: Resolve 14 structural gaps from the Arbor HTR audit, ensuring accurate `relates_to` and `corrects` semantic edges.
4. **Feedback Loop Hardening**: 
   - Add positional correction tracking to bulk ingestion.
   - Trigger LLM Critic and semantic edges in live sessions.
   - Fix wisdom provenance in agent-driven MCP endpoints.
5. **Test Consolidation**: Rely on a comprehensive `test_bootstrap_e2e.rs` and delete overly permissive/redundant tests.
6. **Documentation Alignment**: Synchronize `SKILL.md` with MCP schemas.

## Non-Functional Requirements
- Maintain ingestion latency (<15ms per episode for embeddings).
- Ensure VRAM safety during dreaming (no concurrent Metal GPU context loading).
- Backwards compatibility with existing Mythrax 2.4 datasets.

## Acceptance Criteria
- `test_bootstrap_e2e.rs` passes, proving full `episode -> insight -> direction -> wisdom` pipeline.
- 1000+ transcripts can be ingested without LLM blocks.
- Cloud Brain can asynchronously pull cognitive callbacks to synthesize summaries.
- No `#[allow(dead_code)]` or redundant test files remain.
