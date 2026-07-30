# Implementation Plan: Atomic Insight Itemization & Full-Pipeline Arbor Alignment

## Phase 1: Core Atomic Extraction (Minimum Viable)

- [x] Task: `AtomicInsightItem` struct definition & `ClusterAnalysis` schema update (synthesis.rs L1290-1365)
  - [x] Define `AtomicInsightItem` struct with fields: `title`, `item_type`, `what_was_tried`, `what_happened`, `why_it_happened`, `actionable_takeaway`, `confidence_score`
  - [x] Implement `Validate` trait for `AtomicInsightItem`: validate title non-empty, item_type valid enum value, content non-empty, confidence in [0.0, 1.0]
  - [x] Update `ClusterAnalysis` struct: replace single string/fields with `pub items: Vec<AtomicInsightItem>`
  - [x] Add unit test: verify `AtomicInsightItem` validation succeeds for valid items, fails for invalid item_type or empty fields
  - [x] Add unit test: verify empty items array returns validation error

- [x] Task: LLM system prompt rewrite for atomic itemization (synthesis.rs L1370-1420)
  - [x] Rewrite `SYNTHESIS_SYSTEM_PROMPT` to enforce returning JSON array of `AtomicInsightItem` objects
  - [x] System prompt must explicitly instruct model to split compound findings into separate items
  - [x] Add prompt instructions for classifying each item into item_type: `lesson_learned`, `design_pattern`, `constraint`, `failure_mode`
  - [x] Add unit test: verify system prompt contains item_type guidelines and JSON schema structure

- [x] Task: Per-item WikiNode creation — multi-episode cluster path, data path (synthesis.rs L1476-1564)
  - [x] Loop over `analysis.items` (instead of treating entire analysis as single item)
  - [x] Generate content-derived embedding per item: `embedder.embed(&format!("{}: {} - {}", item.item_type, item.title, item.actionable_takeaway))`
  - [x] Populate `item_type` on `WikiNode` from `item.item_type`
  - [x] Set vault path to `wiki/{scope}/insights/{item_type}/{clean_title}.md`

- [x] Task: Per-item WikiNode creation — multi-episode cluster path, integration wiring (synthesis.rs L1476-1564)
  - [x] Implement deduplication check: cosine similarity > 0.92 against existing nodes in same scope+item_type
  - [x] Create `relates_to` edges from source episode IDs to each wiki_node
  - [x] Call `promote_insight_to_direction` for each saved node
  - [x] Update `insights_changed` counter per item (not per cluster) for compaction trigger
  - [x] Add unit test: verify deduplication merges high-similarity items
  - [x] Add unit test: verify deduplication does NOT merge items with different `item_type` even at high similarity

- [x] Task: Per-item WikiNode creation — single-episode incremental path (synthesis.rs L1173-1268)
  - [x] Remove centroid call at L1043: `calculate_centroid(&ins.source_episodes, &chunk_unprocessed)`
  - [x] Parse LLM response using same `ClusterAnalysis` struct with `items` array
  - [x] Replace monolithic WikiNode creation at L1211-1221 with per-item loop
  - [x] Create per-item WikiNodes with content-derived embeddings (same pattern as cluster path)
  - [x] Ensure `relates_to` edges and `promote_insight_to_direction` calls operate per-item
  - [x] Add unit test: verify single-episode path produces atomic items

- [x] Task: Phase 1 Verification & Checkpoint (Refer to workflow.md)

## Phase 2: Pipeline Propagation

- [x] Task: Update cluster compaction to preserve atomicity (compactor.rs L1004-1075)
  - [x] Replace free-text summary LLM prompt with atomic itemization prompt (same as synthesis)
  - [x] Input structured `items` from child wiki_nodes instead of raw content concatenation
  - [x] Output new `AtomicInsightItem` entries at higher abstraction level
  - [x] Create per-item wiki_nodes from compaction output
  - [x] Mark input wiki_nodes as `node_type: "archived"` after successful compaction
  - [x] Generate content-derived embeddings synchronously (fix `embedding: None` at L1117)
  - [x] Add unit test: verify compaction produces atomic items, not monolithic summary
  - [x] Add unit test: verify compacted nodes have non-null embeddings

- [x] Task: Update outlier compaction to preserve atomicity (compactor.rs L1162-1300)
  - [x] Replace free-text outlier summary prompt at L1175-1176 with atomic itemization prompt
  - [x] Parse output into `AtomicInsightItem` entries
  - [x] Create per-item wiki_nodes with content-derived embeddings (not single miscellaneous node)
  - [x] Replace monolithic WikiNode creation at L1264-1274 with per-item loop
  - [x] Mark source outlier wiki_nodes as `node_type: "archived"` after compaction
  - [x] Add unit test: verify outlier compaction produces atomic items
  - [x] Add unit test: verify outlier compacted nodes have non-null embeddings

- [x] Task: Update wisdom graduation to use `item_type` routing (synthesis.rs L3226-3337)
  - [x] Inspect `node.item_type` in `promote_insight_to_direction` before promotion
  - [x] Route `failure_mode` → WisdomRule with `action_to_avoid`
  - [x] Route `constraint` → WisdomRule with `target_pattern` and `prescribed_remedy`
  - [x] Route `pattern`/`lesson` → positive direction node (current behavior)
  - [x] Include `item_type` in direction node metadata for downstream queryability
  - [x] Add unit test: verify failure_mode produces WisdomRule with action_to_avoid
  - [x] Add unit test: verify pattern produces positive direction node
  - [x] Note: Cross-scope graduation at L2057-2250 benefits automatically from atomicity — no code changes needed

- [x] Task: Enrich distillation with structured `causal_insight` extraction (distillation.rs L320-338)
  - [x] Update distillation LLM prompt to extract atomic `causal_insight` items per transcript chunk
  - [x] Parse structured items into episode's `causal_insight` field as JSON array (not flat string)
  - [x] Add unit test: verify distilled episodes contain structured causal_insight JSON

- [x] Task: Phase 2 Verification & Checkpoint (Refer to workflow.md)

## Phase 3: Raw Markdown Asset Mining

- [x] Task: Expand `ingest_artifacts_in_dir` to handle ALL `.md` files (distillation.rs L533-604)
  - [x] Replace hardcoded filename checks with universal `.md` handler for unmatched files
  - [x] For each unmatched `.md` file: read content, run atomic itemization LLM prompt
  - [x] Create individual wiki_node entries per extracted item with proper `item_type`
  - [x] Generate content-derived embeddings per item
  - [x] Preserve existing special handling for `walkthrough.md`, `task.md` as episode saves
  - [x] Upgrade `implementation_plan.md` handling from regex `extract_decisions()` to full LLM atomic extraction
  - [x] Add unit test: verify spec.md files are processed (not ignored)
  - [x] Add unit test: verify walkthrough.md still creates episode (not wiki_node)

- [x] Task: Add LLM extraction to `sync_file_to_db` for markdown assets (watcher.rs L921-930)
  - [x] When non-episode/non-wisdom `.md` file synced, queue for background atomic extraction via CognitiveTask
  - [x] Don't block watcher — create task asynchronously
  - [x] Cognitive task runs atomic itemization prompt and creates per-item wiki_nodes
  - [x] Add unit test: verify watcher queues cognitive task for markdown files

- [x] Task: Retroactive reprocessing of existing vault markdown
  - [x] Add `manage(action="reprocess_markdown")` MCP endpoint
  - [x] Query all wiki_node records where `item_type IS NONE` and `node_type` is NULL or "wiki"
  - [x] For each, read vault file content and run atomic extraction
  - [x] Create new atomic wiki_node entries per extracted item
  - [x] Mark original monolithic node as `node_type: "archived"`
  - [x] Add unit test: verify reprocessing creates atomic items and archives originals

- [x] Task: Direct WisdomRule generation from spec/plan risk sections (distillation.rs)
  - [x] Add `extract_wisdom_from_document()` function
  - [x] Identify risk tables, constraint sections, failure mode descriptions via LLM extraction
  - [x] Generate WisdomRule entries with proper field mapping
  - [x] Save with `generator_name: "document_extraction"` for provenance tracking
  - [x] Call during artifact ingestion for `spec.md`, `*_review.md`, `*_audit.md` files
  - [x] Add unit test: verify WisdomRule generated from spec risk section

- [x] Task: Phase 3 Verification & Checkpoint (Refer to workflow.md)
  - [x] Execute full suite `cargo nextest run -p mythrax-core` with `MYTHRAX_TEST_MOCK=1` (356/356 passed)
  - [x] Execute `scripts/verify_dev50.sh` benchmark gate (R@5=0.9200, nDCG@10=0.7674 passed)
