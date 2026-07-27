# Implementation Plan: Arbor Framework Alignment & Single-Pass Chunked Ingestion

Incorporates findings from 3 adversarial CTO reviews + 1 forensic root-cause investigation + 1 vault/graph UX audit.

---

> [!CAUTION]
> **Why This Has Failed 3 Times**: Previous versions stored 4-field Arbor output in flat `content: String` columns. Schema normalization, not prompt changes, is the fix.

> [!WARNING]
> **Phase Ordering is Load-Bearing**: Build replacements FIRST (Phase 5), then delete dead code (Phase 6).

> [!IMPORTANT]
> **Obsidian Graph UX**: All SurrealDB edges must be represented as vault-relative `[[wikilinks]]` in markdown. No absolute paths, no empty paths, no SurrealDB IDs in frontmatter.

---

## Phase 0: Immediate Safety Clamps

- [ ] **0.1** Add `IS_INGESTING` guard to `sync_file_to_db` (`watcher.rs` L670).
- [ ] **0.2** Restrict `dispatch_batch` (`arbor.rs` L588) to `buffer_unordered(1)`.
- [ ] **0.3** Wire `skip_llm` parameter (`ingestion.rs` L613).
- [ ] **0.4** Verify: `cargo build --release --features mlx`. No panics or OOM.

---

## Phase 1: Chunked Ingestion Engine

- [ ] **1.1** Chunk `save_episodes_batch_db` into 50-item sub-batches.
- [ ] **1.2** Skip-and-continue failure strategy per chunk.
- [ ] **1.3** Batch IDF index updates into set-based SQL.
- [ ] **1.4** Single-pass scan with JSONL timestamp sorting.
- [ ] **1.5** Verify: Ingest 1,000+ episodes. No transaction timeouts.

---

## Phase 2: 4-Field Schema Normalization (ROOT CAUSE FIX)

- [ ] **2.1** Add `hypothesis`, `raw_evidence`, `causal_insight`, `artifact_refs` to `Episode` (`contracts.rs` L98).
- [ ] **2.2** Add same 4 fields to `WikiNode` (`contracts.rs` L551).
- [ ] **2.3** Update SurrealQL schema definitions (`schema.rs`).
- [ ] **2.4** Define `ArborNode` trait with `h_n()`, `r_n()`, `iota_n()`, `mu_n()` accessors.
- [ ] **2.5** Replace distillation system prompt with 4-field contract (`distillation.rs` L159). Parse into discrete fields.
- [ ] **2.6** Extend `enforce_symbol_integrity` for `raw_evidence` and `artifact_refs`.
- [ ] **2.7** Use `causal_insight` as embedding source, not `content`.
- [ ] **2.8** Verify: Summaries have 4 separate fields, embeddings from `causal_insight`.

---

## Phase 3: Obsidian-Compatible Graph Edge Representation

### Wikilink Contract
1. **Vault-relative paths only.** No `/Users/keith/mythrax-vault/...`. No `[[|title]]` empty paths.
2. **Every SurrealDB edge → wikilink.** `relates_to`, `followed_by`, `superseded_by`, `mentions` edges all rendered as `[[wikilinks]]` in typed `## Relationships` sections.
3. **Backlinks.** Episodes get `## Synthesized Into` → wiki nodes. Wiki nodes get `## Source Episodes` → episodes.
4. **Frontmatter uses wikilink paths**, not SurrealDB record IDs (`episode:uuid`).

### Tasks
- [ ] **3.1** Add `sanitize_wikilink()` helper to `store.rs`. Strip vault root prefix, validate non-empty path.
- [ ] **3.2** Update `synthesis.rs` (L1475-1487): vault-relative wikilinks, `## Relationships` with typed subsections.
- [ ] **3.3** Update `compactor.rs` (L1042-1066): fix absolute path wikilinks, add `## Children` section.
- [ ] **3.4** Update `ingestion.rs`: human-readable episode filenames (slugified title, not UUID). Add `## Synthesized Into` placeholder. Add `## Temporal Navigation` with `followed_by` wikilinks.
- [ ] **3.5** Update `crud_operations.rs`: after `processed_in_dream = true`, patch episode markdown with `## Synthesized Into` backlinks.
- [ ] **3.6** Update MOC.md template (`store.rs` L75-85) to include `[[wiki/|Wiki Knowledge Base]]` with per-scope links.
- [ ] **3.7** Verify: `grep -rn '\[\[|' ~/mythrax-vault/` = 0. `grep -rn '\[\[/Users' ~/mythrax-vault/` = 0. `grep -rn 'episode:[0-9a-f]' ~/mythrax-vault/wiki/` = 0 in frontmatter.

---

## Phase 4: Coordinator MCP Tool Boundaries

- [ ] **4.1** Create `arbor_handlers.rs` with `TreeAddNode`, `TreeUpdateNode`, `TreePrune`, `TreeView(5 formats)`, `GitMergeBranch`.
- [ ] **4.2** All tree mutations update SurrealDB AND vault markdown with proper `[[wikilinks]]`.
- [ ] **4.3** Rewire `handle_pre_invocation_hook` (L1790) to `TreeView(format="constraints")`.
- [ ] **4.4** Verify: All 6 MCP tools callable. Constraints flow to pre-invocation hook.

---

## Phase 5: TreePropagate & Negative Constraints (BUILD FIRST)

- [ ] **5.1** Define `TreePropagate` trait in `arbor.rs`.
- [ ] **5.2** Implement for `HypothesisNode`.
- [ ] **5.3** Update `compact_scope` in `compactor.rs`: keep DBSCAN, add negative constraint extraction.
- [ ] **5.4** Update parent vault markdown with `## Propagated Insights` wikilinks.
- [ ] **5.5** Verify: Parent nodes have abstracted child insights. Negative constraints in pre-invocation hook.

---

## Phase 6: Dead Code Removal (AFTER Phase 4 & 5)

- [ ] **6.1** Delete `backpropagate_insights` (`arbor.rs` L458-514).
- [ ] **6.2** Delete `decide_admission` (`arbor.rs` L516-585).
- [ ] **6.3** Delete `collect_policy_context` (`manage_handlers.rs` L2196-2300).
- [ ] **6.4** Refactor `LlmCriticEvaluator` (`arbor.rs` L96-145): native async.
- [ ] **6.5** Verify: `cargo build --release --features mlx`. All tests pass.

---

## Phase 7: FSM, Convergence & Budget

- [ ] **7.1** `ConvergenceDetector`: sliding window of 5, $\Delta S / \Delta V$, escalating signals.
- [ ] **7.2** `parent_exhaustion` detection.
- [ ] **7.3** FSM: `IDEATE → EXECUTE → EVALUATE → PRUNE/MERGE`.
- [ ] **7.4** `max_depth` (default: 2), budget tracking (token/wall-clock/iteration).
- [ ] **7.5** Verify: FSM enforced, convergence triggers paradigm_shift.

---

## Phase 8: Post-Ingestion Compaction & Vault Cleanup

- [ ] **8.1** After `bulk_ingest_vault` completes, auto-trigger scope compaction for all affected scopes.
- [ ] **8.2** After compaction, physically move archived episodes to `archive/`.
- [ ] **8.3** Regenerate MOC.md with wiki/ scope links.
- [ ] **8.4** Verify: Episodes archived, MOC.md links to wiki, Obsidian graph shows clean wiki clusters.

---

## Phase 9: Integration & Ship

- [ ] **9.1** Full integration: 1,000+ episodes → dreaming → Arbor HTR → all systems.
- [ ] **9.2** Obsidian verification: Open vault, verify graph view shows wiki clusters not episode noise.
- [ ] **9.3** Full test suite: `MYTHRAX_TEST_MOCK=1 cargo nextest run -p mythrax-core`.
- [ ] **9.4** Git commit and push.
