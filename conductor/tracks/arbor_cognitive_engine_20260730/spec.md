# Track Specification: Arbor-Aligned Cognitive Memory Engine Replacement (v1.1)

## Overview
Replace ~8,000 lines of fragmented, monolithic cognitive code (`synthesis.rs`, `compactor.rs`, `critic.rs`, `meta_skill.rs`, `harvest.rs`) with ~1,200 lines implementing an Arbor-aligned knowledge pipeline: **Extract → Cluster → Hypothesize → Test → Refine → Merge → Graduate.**

## Functional Requirements
1. **Arbor Triplet Node Schema ($h_n, \iota_n, r_n, \mu_n$):** Every `Fact`/`Extract` node carries $h_n$ (hypothesis), $\iota_n$ (causal insight), $r_n$ (raw evidence), and $\mu_n$ (artifact/code references) implementing the `ArborNode` trait. `Episode::causal_insight` is updated to a typed JSON array (`Option<serde_json::Value>`).
2. **5 Fact Ingestion Entrypoints:** Support continuous extraction across `Episode` (raw turns), `Document` (authored vault docs), `Code` (workspace source files), `ForgedDocument` (PDFs/papers), and `Skill` (`SKILL.md` playbooks).
3. **Dual-Path Vault Wiki Integration:**
   - **Path A (Raw Reference Pages):** Write raw section chunks/skills to `/wiki/{scope}/forge/` and `/wiki/skills/`, indexed as `WikiNode` records for instant vector + BM25 search.
   - **Path B (Synthesized Pages):** Extract Arbor triplets ($h_n, \iota_n, \mu_n$) to feed downstream HTR clustering and synthesis.
4. **Greedy Cosine Clustering & Content-Derived Embeddings:** Group unassociated facts by cosine similarity ($\ge 0.75$, min size 3) using content-derived embeddings (`backend.embed_batch()`), completely eliminating centroid vector math.
5. **HTR Engine Lifecycle & Held-Out Admission Gate:**
   - `form_hypotheses()`: Injects negative policy constraints gathered via `collect_policy_context()`.
   - `refine_hypotheses()`: Evaluates new facts against claims; support $\rightarrow$ confidence $\uparrow$, contradict $\rightarrow$ confidence $\downarrow$. Triggers GC sweep for 0-degree pruned nodes ($\le 0.20$).
   - `merge_validated_nodes()`: Runs `HeldOutEvaluator` / `TestCommandEvaluator` (`Etest` git worktree merge gate) for code hypotheses, then performs LLM ancestor synthesis (~500–2,000 tokens) while flattening & preserving child $r_n$ evidence and $\mu_n$ references.
   - `graduate()`: Promotes universal claims to `scope: "general"`.
6. **Hook Architecture & Vault Remediation:**
   - `stop.rs`: Queue background extractions through bounded `CognitiveTask` table.
   - `precompact.rs`: Direct user correction contradiction demotions (`refine_hypotheses()`).
   - `watcher.rs` & `distillation.rs`: Delete legacy regex functions (`extract_wisdom_from_document`, `extract_decisions`), routing via `CognitiveTask` table to `pipeline::extract_from_document()`.
   - `vault_handlers.rs`: Extend `reprocess_markdown` to vault-wide directory iteration.
   - `reflect.rs`: Inject pruned negative constraints ($\le 0.20$) and validated working context ($\ge 0.90$).

## Acceptance Criteria
- `MYTHRAX_TEST_MOCK=1 cargo nextest run -p mythrax-core domain_cognitive` passes 100%.
- All 5 fact sources produce valid Arbor triplets in SurrealDB `fact` and `Episode::causal_insight` persists as JSON arrays.
- Legacy regex stubs in `distillation.rs` and `compactor.rs`, `synthesis.rs`, `critic.rs`, `meta_skill.rs` deleted; codebase net reduction ~6,800 lines.
- Held-Out Admission Gate executes in isolated git worktrees `/tmp/admission-gate-<node_id>` during merges.
- `scripts/verify_dev50.sh` benchmark passes without recall or latency regressions.
