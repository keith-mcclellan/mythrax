# Implementation Plan: Arbor Framework Alignment & Single-Pass Chunked Ingestion

Incorporates findings from 3 adversarial CTO reviews + 1 forensic root-cause investigation + 1 vault/graph UX audit + 5 rounds of deep architectural investigation. **20 Phase -1 emergency tasks** before any Arbor work.

---

> [!CAUTION]
> ## 🚨 THE ACTUAL ROOT CAUSE — THREE FATAL STRING-MATCHING BUGS
> 
> The deep investigation found the entire memory system is non-functional due to three bugs in `manage_handlers.rs`:
>
> **Bug 1: Guardrail rules never fire (L1496-1498)**: `.contains()` exact substring match means rules are only injected if the agent already said the pattern. Catch-22 — agent can't be warned about what it doesn't know.
>
> **Bug 2: Auto-retrieval searches "general context" (L1727)**: Fallback query returns noise instead of task-relevant memories.
>
> **Bug 3: Utilization scoring evicts good memories (L1411-1447)**: Same `.contains()` check for utilization. When it fails, EMA decays importance → memory evicted. System punishes correct memories for a UX bug.

> [!WARNING]
> **Phase Ordering is Load-Bearing**: Fix the integration bugs FIRST (Phase -1). Build replacements BEFORE deleting dead code (Phase 5 before Phase 6).

> [!IMPORTANT]
> **Obsidian Graph UX**: All SurrealDB edges must be represented as vault-relative `[[wikilinks]]` in markdown.

---

## Phase -1: EMERGENCY — Fix the Three Fatal Memory Integration Bugs

These must be fixed BEFORE any other work. Without them, nothing else matters.

- [x] **-1.1** Replace guardrail `.contains()` with semantic similarity (`manage_handlers.rs` L1496-1518)
- [x] **-1.2** Replace `"general context"` fallback with actual task context (`manage_handlers.rs` L1727)
- [x] **-1.3** Fix utilization scoring — stop evicting good memories (`manage_handlers.rs` L1411-1466)
- [x] **-1.4** Fix corrupted wisdom graduation (`synthesis.rs` L3468-3469)
- [x] **-1.5** Fix distillation prompt — extract mistakes (`distillation.rs` L289-295)
- [x] **-1.6** Fix correction detection — replace keyword matching (`precompact.rs` L300-308)
- [x] **-1.7** Fix token budget silent eviction (`manage_handlers.rs` L1262-1327)
- [x] **-1.8** Implement post-invocation hook (`manage_handlers.rs` L1159)
- [x] **-1.9** Fix `p1_advisory.clear()` — the nuclear memory wipe (`manage_handlers.rs` L1838-1841)
- [x] **-1.10** Fix embedding content — stop embedding noise (`daemon.rs` L190, L237)
- [x] **-1.11** Fix search result formatting — stop returning raw JSON (`read_handlers.rs` L270)
- [x] **-1.12** Fix `let _ =` silent error swallowing across all critical paths
- [x] **-1.13** Fix TOCTOU race in arbor backpropagation (`arbor.rs` `backpropagate_insights`)
- [x] **-1.14** Fix STM handoff truncation (`manage_handlers.rs` L98)
- [x] **-1.15** Fix RAPTOR embedding gap (`compactor.rs` L1536)
- [x] **-1.16** Fix wisdom extraction trap — add `save_wisdom` MCP action (`write_handlers.rs` L15)
- [x] **-1.17** Add missing embedding reprocessor (`daemon.rs` L172, L218)
- [x] **-1.18** Fix cognitive task queue polling bottleneck (`distillation.rs` L345)
- [x] **-1.19** Add MCP route handler integration tests (`tests/domain_hooks_models.rs`)
- [x] **-1.20** FINAL VERIFICATION — end-to-end test of entire memory lifecycle (`tests/domain_e2e_harness.rs`)
  3. Verify importance does NOT decay (injected = utilized)
  4. Verify graduated rules have correct `action_to_avoid` ≠ `target_pattern`
  5. Verify distilled episodes contain "Mistakes & Failures" section
  6. Verify search results return readable markdown (not escaped JSON)
  7. Verify STM passes large payloads between agents
  8. Verify RAPTOR summaries have embeddings (reprocessor catches `None`)
  9. Verify pre-invocation with 50k tokens of context doesn't nuclear-wipe memories
  10. Verify bulk summarization doesn't spawn hundreds of polling loops

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

## Phase 2: 4-Field Schema Normalization (STRUCTURAL ROOT CAUSE FIX)

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
1. Vault-relative paths only. No absolute paths. No `[[|title]]` empty paths.
2. Every SurrealDB edge → wikilink. Typed `## Relationships` sections.
3. Backlinks: episodes → wiki nodes. Wiki nodes → source episodes.
4. Frontmatter uses wikilink paths, not SurrealDB record IDs.

### Tasks
- [ ] **3.1** Add `sanitize_wikilink()` helper to `store.rs`.
- [ ] **3.2** Update `synthesis.rs`: vault-relative wikilinks, typed relationship sections.
- [ ] **3.3** Update `compactor.rs`: fix absolute path wikilinks, add `## Children`.
- [ ] **3.4** Update `ingestion.rs`: human-readable filenames (slugified title). `## Synthesized Into` placeholder. `## Temporal Navigation` wikilinks.
- [ ] **3.5** Update `crud_operations.rs`: after `processed_in_dream = true`, patch episode markdown with backlinks.
- [ ] **3.6** Update MOC.md template to include wiki/ scope links.
- [ ] **3.7** Verify: `grep '\[\[|'` = 0. `grep '\[\[/Users'` = 0. `grep 'episode:[0-9a-f]'` in wiki frontmatter = 0.

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

- [ ] **8.1** After `bulk_ingest_vault` completes, auto-trigger scope compaction.
- [ ] **8.2** After compaction, physically move archived episodes to `archive/`.
- [ ] **8.3** Regenerate MOC.md with wiki/ scope links.
- [ ] **8.4** Verify: Episodes archived, MOC.md links to wiki, Obsidian graph clean.

---

## Phase 9: Integration & Ship

- [ ] **9.1** Full integration: 1,000+ episodes → dreaming → Arbor HTR → all systems.
- [ ] **9.2** CRITICAL TEST: Create wisdom rule → new session → rule appears WITHOUT agent mentioning it → importance does NOT decay.
- [ ] **9.3** Obsidian verification: Open vault, graph view shows wiki clusters not episode noise.
- [ ] **9.4** Full test suite: `MYTHRAX_TEST_MOCK=1 cargo nextest run -p mythrax-core`.
- [ ] **9.5** Git commit and push.
