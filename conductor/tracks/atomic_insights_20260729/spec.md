# Specification: Atomic Insight Itemization & Full-Pipeline Arbor Alignment

## Overview

Mythrax's insight generation pipeline produces shallow, 1-sentence activity summaries instead of deep causal architectural principles. Per Arbor §4.2, insights (ι_n) must be "NOT an execution transcript, but a compact semantic memory" answering "what was tried, what happened, and why." Ablation data (§5.7, Table 4) proves ι_n quality is the #1 system factor — disabling insight feedback causes a 27.28 percentage point performance drop.

This track atomizes insight extraction across the full pipeline: synthesis, compaction, wisdom graduation, distillation, and raw markdown asset ingestion. Each insight cluster produces multiple individually-vectorized atomic items (patterns, constraints, failure modes, lessons) instead of a single monolithic summary.

## Functional Requirements

### FR-1: Atomic Insight Extraction
- Synthesis must extract multiple `AtomicInsightItem` entries per cluster, each with `title`, `item_type` (pattern|constraint|failure_mode|lesson), `content`, and `metacognitive_confidence`.
- Each item must have its own content-derived 768-dim MLX embedding (NOT centroid).
- Items are stored as individual WikiNodes with `item_type` field.
- Maximum 5 items per cluster. Minimum 100-char content per item.

### FR-2: Dual-Path Synthesis Coverage
- The multi-episode DBSCAN cluster path (synthesis.rs L1476-1564) must produce atomic items.
- The single-episode incremental path (synthesis.rs L1173-1268) must produce atomic items.
- Both centroid call sites (L1043, L1504) must be removed.

### FR-3: Compactor Atomicity Preservation
- Cluster compaction (compactor.rs L1004-1075) must re-extract atomic items at a higher abstraction level, not produce free-text summaries.
- Outlier compaction (compactor.rs L1162-1300) must receive the same treatment.
- All compaction nodes must have synchronous content-derived embeddings (no `embedding: None`).

### FR-4: Wisdom Graduation Routing
- `promote_insight_to_direction` must inspect `item_type` and route:
  - `failure_mode` → WisdomRule with `action_to_avoid`
  - `constraint` → WisdomRule with `target_pattern` + `prescribed_remedy`
  - `pattern`/`lesson` → Positive direction node or WisdomRule

### FR-5: Distillation Enrichment
- Transcript distillation must extract structured `causal_insight` items into the episode's `causal_insight` field as a JSON array.

### FR-6: Raw Markdown Asset Mining
- `ingest_artifacts_in_dir` must handle ALL `.md` files, not just 4 hardcoded patterns.
- Filesystem watcher must queue non-episode/non-wisdom markdown for background atomic extraction.
- A `manage(action="reprocess_markdown")` endpoint must retroactively process existing vault markdown.
- Spec/plan risk sections must generate WisdomRules directly.

## Non-Functional Requirements

### NFR-1: Backward Compatibility
- `ClusterAnalysis` must support fallback parsing: if `items` is empty but `title`+`summary` are present, wrap into a single `AtomicInsightItem`.

### NFR-2: Quality Gates
- Reject items with `content.len() < 100` characters.
- Reject items starting with passive-voice verbs.
- Log rejected items for audit.

### NFR-3: Deduplication
- Before saving, check cosine similarity > 0.92 against existing wiki_nodes in same scope with same `item_type`. Merge rather than duplicate.

### NFR-4: DBSCAN Tuning
- After Phase 1 deployment, validate `final_eps` values against new content-derived embedding distribution.

## Acceptance Criteria

1. Running `manage(action="summarize", scope="all")` produces wiki_node entries in `wiki/<scope>/insights/<item_type>/` subdirectories.
2. Each insight note contains ≥2 sentences of causal content (not activity summaries).
3. WikiNodes have content-derived embeddings (verified via SurrealDB query, not centroids).
4. Compaction preserves atomic items (not re-merged into monolithic blobs).
5. Wisdom graduation routes `failure_mode` items to `action_to_avoid` WisdomRules.
6. All `.md` files in artifact directories are processed (not just 4 hardcoded patterns).
7. `manage(action="reprocess_markdown")` successfully atomizes existing vault markdown.
8. All domain test suites pass: `domain_cognitive`, `domain_vault_storage`.
9. dev50 benchmark does not regress below baseline.

## Out of Scope

- Changes to the DBSCAN clustering algorithm itself (it works correctly; the input embeddings were the problem).
- Changes to the ArborNode trait, ConvergenceDetector, TreePropagate, or hierarchical DBSCAN.
- Changes to the interleaved compaction trigger logic.
- Transcript chunking strategy changes (tool-call boundaries → semantic boundaries is a separate future track).
