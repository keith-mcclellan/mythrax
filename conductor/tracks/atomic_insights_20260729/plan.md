# Implementation Plan: Atomic Insight Itemization & Full-Pipeline Arbor Alignment

## Phase 1: Core Atomic Extraction (Minimum Viable)

- [ ] Task: Define `AtomicInsightItem` struct and update `ClusterAnalysis` (synthesis.rs ~L1434)
  - [ ] Define `AtomicInsightItem` struct with `title`, `item_type`, `content`, `metacognitive_confidence`
  - [ ] Update `ClusterAnalysis` to include `items: Vec<AtomicInsightItem>` with backward-compat fallback fields
  - [ ] Add fallback parsing: if `items` is empty but `title`+`summary` present, wrap into single item with `item_type: "lesson"`
  - [ ] Add unit test: verify `ClusterAnalysis` deserialization with `items` array
  - [ ] Add unit test: verify fallback parsing for legacy format
  - [ ] Add unit test: verify `ClusterAnalysis` handles empty `items` AND missing fallback fields gracefully (no panic, logs warning, produces zero items)

- [ ] Task: Add `item_type` field to WikiNode schema
  - [ ] Add `pub item_type: Option<String>` to `WikiNode` struct in contracts.rs (L636)
  - [ ] Update SurrealDB table definition in schema.rs
  - [ ] Update `save_wiki_node` in crud_operations.rs to include `item_type` in INSERT/UPDATE
  - [ ] Update `get_wiki_nodes_paginated` in crud_operations.rs to include `item_type` in SELECT
  - [ ] Update `get_memory_nodes` in crud_operations.rs to include `item_type` in SELECT for wiki_nodes
  - [ ] Add unit test: verify `item_type` round-trips through save/load
  - [ ] Add unit test: verify `item_type` preserved through `save_wiki_node_with_contradiction_resolution` merge path

- [ ] Task: Rewrite synthesis system prompt with few-shot examples (synthesis.rs L1399-1414)
  - [ ] Replace current system prompt with Arbor-mandated prompt requiring `items` array with `item_type` classification
  - [ ] Add few-shot examples for each item_type: `pattern`, `constraint`, `failure_mode`, `lesson`
  - [ ] Add cap directive: "Return at most 5 items per cluster"
  - [ ] Add minimum content length guidance: "Each item's content MUST be at least 2-3 sentences"
  - [ ] Update user prompt to show expected JSON response format with `items` array
  - [ ] Implement quality gate: reject items with `content.len() < 100`
  - [ ] Implement quality gate: reject items starting with passive-voice verbs
  - [ ] Add rejected items audit logging
  - [ ] Add unit test: verify quality gate rejects short items
  - [ ] Add unit test: verify quality gate rejects passive-voice items

- [ ] Task: Per-item WikiNode creation — multi-episode cluster path, core data path (synthesis.rs L1476-1564)
  - [ ] Remove centroid embedding at L1504: `let centroid = calculate_centroid(...)`
  - [ ] Loop through `analysis.items` to create per-item WikiNodes
  - [ ] Generate slugified filename per item: `wiki/<scope>/insights/<item_type>/<slug>.md`
  - [ ] Write individual Obsidian markdown note with frontmatter including `item_type`
  - [ ] Compute content-derived embedding: embed `format!("{}: {}", item.title, item.content)`
  - [ ] Create WikiNode per item with `item_type`, `node_type: "insight"`, content-derived embedding
  - [ ] Save via `save_wiki_node_with_contradiction_resolution`
  - [ ] Add unit test: verify multiple wiki_nodes created from multi-item analysis

- [ ] Task: Per-item WikiNode creation — multi-episode cluster path, integration wiring (synthesis.rs L1476-1564)
  - [ ] Implement deduplication check: cosine similarity > 0.92 against existing nodes in same scope+item_type
  - [ ] Create `relates_to` edges from source episode IDs to each wiki_node
  - [ ] Call `promote_insight_to_direction` for each saved node
  - [ ] Update `insights_changed` counter per item (not per cluster) for compaction trigger
  - [ ] Add unit test: verify deduplication merges high-similarity items
  - [ ] Add unit test: verify deduplication does NOT merge items with different `item_type` even at high similarity

- [ ] Task: Per-item WikiNode creation — single-episode incremental path (synthesis.rs L1173-1268)
  - [ ] Remove centroid call at L1043: `calculate_centroid(&ins.source_episodes, &chunk_unprocessed)`
  - [ ] Parse LLM response using same `ClusterAnalysis` struct with `items` array
  - [ ] Replace monolithic WikiNode creation at L1211-1221 with per-item loop
  - [ ] Create per-item WikiNodes with content-derived embeddings (same pattern as cluster path)
  - [ ] Ensure `relates_to` edges and `promote_insight_to_direction` calls operate per-item
  - [ ] Add unit test: verify single-episode path produces atomic items

- [ ] Task: Phase 1 Verification & Checkpoint (Refer to workflow.md)

## Phase 2: Pipeline Propagation

- [ ] Task: Update cluster compaction to preserve atomicity (compactor.rs L1004-1075)
  - [ ] Replace free-text summary LLM prompt with atomic itemization prompt (same as synthesis)
  - [ ] Input structured `items` from child wiki_nodes instead of raw content concatenation
  - [ ] Output new `AtomicInsightItem` entries at higher abstraction level
  - [ ] Create per-item wiki_nodes from compaction output
  - [ ] Mark input wiki_nodes as `node_type: "archived"` after successful compaction
  - [ ] Generate content-derived embeddings synchronously (fix `embedding: None` at L1117)
  - [ ] Add unit test: verify compaction produces atomic items, not monolithic summary
  - [ ] Add unit test: verify compacted nodes have non-null embeddings

- [ ] Task: Update outlier compaction to preserve atomicity (compactor.rs L1162-1300)
  - [ ] Replace free-text outlier summary prompt at L1175-1176 with atomic itemization prompt
  - [ ] Parse output into `AtomicInsightItem` entries
  - [ ] Create per-item wiki_nodes with content-derived embeddings (not single miscellaneous node)
  - [ ] Replace monolithic WikiNode creation at L1264-1274 with per-item loop
  - [ ] Mark source outlier wiki_nodes as `node_type: "archived"` after compaction
  - [ ] Add unit test: verify outlier compaction produces atomic items
  - [ ] Add unit test: verify outlier compacted nodes have non-null embeddings

- [ ] Task: Update wisdom graduation to use `item_type` routing (synthesis.rs L3226-3337)
  - [ ] Inspect `node.item_type` in `promote_insight_to_direction` before promotion
  - [ ] Route `failure_mode` → WisdomRule with `action_to_avoid`
  - [ ] Route `constraint` → WisdomRule with `target_pattern` and `prescribed_remedy`
  - [ ] Route `pattern`/`lesson` → positive direction node (current behavior)
  - [ ] Include `item_type` in direction node metadata for downstream queryability
  - [ ] Add unit test: verify failure_mode produces WisdomRule with action_to_avoid
  - [ ] Add unit test: verify pattern produces positive direction node
  - [ ] Note: Cross-scope graduation at L2057-2250 benefits automatically from atomicity — no code changes needed

- [ ] Task: Enrich distillation with structured `causal_insight` extraction (distillation.rs L320-338)
  - [ ] Update distillation LLM prompt to extract atomic `causal_insight` items per transcript chunk
  - [ ] Parse structured items into episode's `causal_insight` field as JSON array (not flat string)
  - [ ] Add unit test: verify distilled episodes contain structured causal_insight JSON

- [ ] Task: Phase 2 Verification & Checkpoint (Refer to workflow.md)

## Phase 3: Raw Markdown Asset Mining

- [ ] Task: Expand `ingest_artifacts_in_dir` to handle ALL `.md` files (distillation.rs L533-604)
  - [ ] Replace hardcoded filename checks with universal `.md` handler for unmatched files
  - [ ] For each unmatched `.md` file: read content, run atomic itemization LLM prompt
  - [ ] Create individual wiki_node entries per extracted item with proper `item_type`
  - [ ] Generate content-derived embeddings per item
  - [ ] Preserve existing special handling for `walkthrough.md`, `task.md` as episode saves
  - [ ] Upgrade `implementation_plan.md` handling from regex `extract_decisions()` to full LLM atomic extraction
  - [ ] Add unit test: verify spec.md files are processed (not ignored)
  - [ ] Add unit test: verify walkthrough.md still creates episode (not wiki_node)

- [ ] Task: Add LLM extraction to `sync_file_to_db` for markdown assets (watcher.rs L921-930)
  - [ ] When non-episode/non-wisdom `.md` file synced, queue for background atomic extraction via CognitiveTask
  - [ ] Don't block watcher — create task asynchronously
  - [ ] Cognitive task runs atomic itemization prompt and creates per-item wiki_nodes
  - [ ] Add unit test: verify watcher queues cognitive task for markdown files

- [ ] Task: Retroactive reprocessing of existing vault markdown
  - [ ] Add `manage(action="reprocess_markdown")` MCP endpoint
  - [ ] Query all wiki_node records where `item_type IS NONE` and `node_type` is NULL or "wiki"
  - [ ] For each, read vault file content and run atomic extraction
  - [ ] Create new atomic wiki_node entries per extracted item
  - [ ] Mark original monolithic node as `node_type: "archived"`
  - [ ] Add unit test: verify reprocessing creates atomic items and archives originals

- [ ] Task: Direct WisdomRule generation from spec/plan risk sections (distillation.rs)
  - [ ] Add `extract_wisdom_from_document()` function
  - [ ] Identify risk tables, constraint sections, failure mode descriptions via LLM extraction
  - [ ] Generate WisdomRule entries with proper field mapping
  - [ ] Save with `generator_name: "document_extraction"` for provenance tracking
  - [ ] Call during artifact ingestion for `spec.md`, `*_review.md`, `*_audit.md` files
  - [ ] Add unit test: verify WisdomRule generated from spec risk section

- [ ] Task: Phase 3 Verification & Checkpoint (Refer to workflow.md)
