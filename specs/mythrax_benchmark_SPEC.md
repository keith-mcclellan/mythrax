# Mythrax Advanced-Memory — Executable Spec

**Audience:** an autonomous coding agent operating under spec-driven development (tests first) with AST/symbol grounding.

**How to use this document:**
1. Work one **EPIC** at a time, top to bottom. Each epic is independently shippable and gated.
2. For each epic: **(a)** verify every symbol in `## AST ANCHORS` actually exists at the cited path before writing code — if a signature differs from what's written here, STOP and reconcile against the real source, do not invent. **(b)** Write the tests in `## TESTS (write first)` and confirm they FAIL for the right reason. **(c)** Implement until tests pass. **(d)** Satisfy every item in `## ACCEPTANCE`.
3. Never introduce an API, struct field, trait method, REST route, or SurrealQL table/field that is not either (i) listed in an AST ANCHORS block as *existing*, or (ii) explicitly specified in this doc as *to-be-created* with its full signature.

**Anti-hallucination contract (read once, apply always):**
- All anchors below were extracted from the real source on the `main` branch. Line numbers are advisory (drift over time); **symbol names and signatures are authoritative**. Resolve symbols by name, not by line.
- Before calling any function, open its definition and confirm arity + types. If your harness has `ast-grep`/`rust-analyzer`/`pyright`, run the verification commands in `## SYMBOL VERIFICATION` for each epic.
- If a required symbol does not exist, treat it as a spec defect: emit a `SPEC-GAP:` note and halt that epic rather than fabricating the symbol.

**Target repo (the ONLY repo this spec touches):**
- Mythrax: `github.com/keith-mcclellan/mythrax`, crate root `mythrax-core/`. **Verified against the `2.0.0` release (commit `bc9e282c`, merge `ab0d5f85`, 2026-06-26).** Rust, Axum, **dual-engine** SurrealDB (`surrealkv://` or `rocksdb://`) + HNSW (DIMENSION 768 DIST COSINE), thread-safe WAL, daemon on port 8090 (REST + JSON-RPC MCP + OpenAI completions proxy on 8080).

This spec is **self-contained**: every algorithm, formula, weight, and parameter you need is written out below in plain terms. No *memory-project* repositories are to be read, fetched, or cloned. Implement everything natively in Rust against Mythrax's existing types. (The only sanctioned external artifacts are the official benchmark *datasets* and — for SWE-bench — the benchmark's *own* official scorer, used at arm's length for honest, publishable results; see the CODE vs DATA vs OFFICIAL SCORER exception.)

> **GROUND RULE — SELF-CONTAINED, RUST-NATIVE ONLY (non-negotiable, applies to every epic).**
> Build every capability below **natively in idiomatic Rust** against Mythrax's existing types. This document is the complete and only source.
> - **Do not fetch, clone, read, vendor, or depend on any external memory project.** No `Cargo.toml` entry, git submodule, vendored source tree, FFI, subprocess into a *memory-project* CLI, or network call to an outside *memory* service. Mythrax stays a self-contained Rust daemon. (The one sanctioned subprocess is the benchmark's *own* official scorer in Epic 11B — see the CODE vs DATA vs OFFICIAL SCORER exception below; it is never linked into the daemon binary.)
> - Every formula, weight, threshold, and parameter you need is written out in this spec. If something seems to require reading an outside codebase, that is a spec defect: emit `SPEC-GAP:` and stop the epic rather than going to fetch external code.
> - **CODE vs DATA vs OFFICIAL SCORER — the explicit exceptions (for honest, publishable benchmarks only).** The "no external code" rule targets *memory-project* code (the design sources we are merely harvesting ideas from). It carves out exactly two things, and nothing else:
>   1. **Benchmark datasets** — the official, public, citable datasets the published metrics are defined over. Epic 1 (LongMemEval) and Epic 11B (SWE-bench Verified) acquire their data **only** from the canonical primary sources named in this spec (the original authors' published releases), pinned by exact dataset id + revision + checksum, used as read-only evaluation input.
>   2. **The benchmark's OWN official scoring harness** — where a benchmark *defines correctness via its authors' reference scorer* (notably SWE-bench's apply-patch-and-run-tests "% Resolved" judge), running that official harness is the **most honest** option and is explicitly allowed. Re-implementing the scorer ourselves would make results non-comparable and is forbidden for such benchmarks. The official harness is used at arm's length: invoked as an out-of-process tool (subprocess/CLI), pinned to a recorded version, never vendored into the daemon, never linked into the Mythrax binary.
>   Everything else stays native Rust: all metrics we *can* define unambiguously ourselves (LongMemEval Recall@k / nDCG@k per REFERENCE BEHAVIORS) are implemented natively, not pulled from any repo. And the forbidden category is unchanged: **no code, scripts, or logic from any memory project** (the idea-source repos) may be fetched, vendored, read, or executed. Sourcing benchmark data from anywhere other than the named canonical source, pulling any *memory-project* code, or substituting a re-implemented scorer where an official one is the standard, is a defect: emit `SPEC-GAP:` and stop.
> - The numeric constants and metric definitions in this spec (BM25 `k1`/`b`, fusion weights, boost ladders, `SAVE_INTERVAL`, `CHARS_PER_TOKEN`, recall/nDCG definitions, etc.) are standard techniques and plain parameters — implement them directly in Rust.
> - The benchmark datasets (Epic 1 LongMemEval, Epic 11B SWE-bench Verified) come **only** from the canonical primary sources named in Epic 1 / Epic 11B and the PROVENANCE block, pinned by id+revision+checksum (see CODE vs DATA exception above). Do not source benchmark data from any third-party / reference-implementation repo; do not silently substitute a derived or pre-split copy.

> **ARCHITECTURE DELTA — re-verified 2026-06-26 against Mythrax 2.0.0.** This spec was re-aligned after a major refactor. Key structural changes that affect the epics below:
> - **MCP surface consolidated** from ~9 flat tools into **7 verb-router tools** in the NEW module `mcp_routes.rs`: `manage_memory`, `manage_htr`, `manage_stm`, `manage_vault`, `manage_config`, `pre_invocation_hook`, `manage_file`. Each dispatches on an inner `action` string. **New MCP capabilities are added as `action` variants on an existing verb tool — NOT as new flat tools.**
> - `pre_invocation_hook` is now a first-class MCP tool (`handle_pre_invocation_hook` in `mcp_routes.rs`, ~line 1273), not just a read-path concept.
> - **New trait methods exist** and should be reused rather than reinvented: `query_symbolic` (graph traversal), `journal_state` (dual-durability journal), `reinforce_episode`, `save_thought_node`, `prune_stale_memories`, `get_checkpoints`. Full list in GLOBAL AST ANCHORS.
> - **New `metrics` table** tracks utility/access out-of-band (`utility_score`, `access_count`, `last_accessed`); a `symbol_archive` table and `query_symbolic` traversal now exist (relevant to Epics 5 & 10).
> - **Compactor archive flow rewritten**: `archive_decayed_episodes` now moves the physical file to `vault/archive/`, generates a RAPTOR summary `wiki_node`, THEN deletes the DB record (`delete_by_vault_path`, compactor.rs ~line 514). Trigger changed from `utility < 5.0` to `decayed_utility < decay_threshold*50.0` (profile key `compaction.decay_threshold`, default 0.15). This reshapes Epic 3 (see its revised premise).
> - `contracts.rs` core structs (`EpisodeSave`/`Episode`/`SearchResult`/`SearchResponse`/`WikiNode`) and the 11-arg `search` signature are **UNCHANGED** — Epics 1–11 anchored on them remain valid.
> - Both prior SPEC-GAPs are now RESOLVED (SurrealDB `array<...>` field syntax confirmed; MCP dispatch located). See Epics 9 & 10.

---

## GLOBAL AST ANCHORS (shared across epics — verify once)

These exist today and are reused throughout. Confirm before Epic 1.

```
# Crate: mythrax-core

contracts.rs
  struct EpisodeSave { title:String, content:String, entities:Vec<Entity>, scope:Option<String>,
                       vault_path:Option<String>, source_episode:Option<String>,
                       session_id:Option<String>, task_id:Option<String> }
  struct Episode { id:Option<String>, title:String, content:String, source:Option<String>,
                   scope:Option<String>, vault_path:Option<String>, embedding:Option<Vec<f32>>,
                   processed_in_dream:Option<bool>, source_episode:Option<String>,
                   last_retrieved_at:Option<String>, utility:Option<f32> }
  struct SearchResult { id:String, title:String, content:String, similarity:f32, utility:f32,
                        tier:String, embedding:Option<Vec<f32>>, vault_path:Option<String>,
                        source_episode:Option<String>, related_nodes:Option<Vec<SearchResult>> }
  struct SearchResponse { results:Vec<SearchResult>, total_matches:usize, has_more:bool,
                          next_offset:usize, omitted_ids:Option<Vec<String>> }
  struct WikiNode { id:Option<String>, name:String, content:String, scope:String,
                    vault_path:Option<String>, embedding:Option<Vec<f32>> }
  struct Entity { ... }   // verify fields in contracts.rs before constructing

db/backend.rs  (impl StorageBackend for SurrealBackend)  [trait @ line 102; file ~4362 lines in 2.0.0]
  trait StorageBackend {
    async fn init(&self) -> Result<()>;
    async fn save_episode(&self, episode:&EpisodeSave) -> Result<String>;   // returns "episode:<uuid>"  (@1009)
    async fn save_wisdom_rule(&self, rule:&WisdomRule) -> Result<String>;
    async fn search(&self, query:&str, scope:Option<&str>, deep_insight:bool, limit:usize,    // UNCHANGED 11 args @106
                    offset:usize, threshold:f32, token_budget:Option<usize>, allow_downward:bool,
                    include_episodes:bool, include_artifacts:bool) -> Result<SearchResponse>;  // @1392
    async fn get_wisdom(&self, query:&str, tier:Option<&str>, limit:usize, offset:usize, threshold:f32) -> Result<WisdomSearchResponse>;
    async fn record_feedback(&self, id:&str, success:bool) -> Result<()>;
    async fn get_unprocessed_episodes(&self) -> Result<Vec<Episode>>;
    async fn mark_episode_processed(&self, id:&str) -> Result<()>;
    async fn get_all_episodes(&self) -> Result<Vec<Episode>>;
    async fn save_wiki_node(&self, node:&WikiNode) -> Result<String>;
    async fn relate_nodes(&self, from_id:&str, to_id:&str) -> Result<()>;
    async fn get_related_node_ids(&self, from_id:&str) -> Result<Vec<String>>;          // NEW in 2.0
    async fn get_active_scopes(&self) -> Result<Vec<String>>;
    async fn delete_by_vault_path(&self, vault_path:&str) -> Result<()>;
    async fn get_memory_nodes(&self, node_ids:&[String]) -> Result<GetMemoryNodesResponse>;
    async fn embed(&self, text:&str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts:&[String]) -> Result<Vec<Vec<f32>>>;
    async fn prune_stale_memories(&self, vault_root:&Path) -> Result<()>;               // NEW in 2.0
    async fn reinforce_episode(&self, id:&str) -> Result<()>;                           // NEW in 2.0 (utility bump)
    async fn journal_state(&self, vault_root:&Path, session_id:Option<&str>) -> Result<()>;  // NEW: dual-durability
    async fn get_checkpoints(&self) -> Result<Vec<serde_json::Value>>;
    async fn query_symbolic(&self, node_id:&str, relation:Option<&str>, max_depth:Option<usize>) -> Result<Vec<String>>;  // NEW: graph traversal (@158)
    async fn save_thought_node(&self, thought:&ThoughtNode) -> Result<String>;          // NEW in 2.0
    fn as_any(&self) -> &dyn std::any::Any;   // (~25 methods total; list above is the epic-relevant subset)
  }
  // SurrealBackend has pub field `db` (used in tests via backend.db.query(...).bind(...))
  // SurrealBackend::new_in_memory() -> Result<SurrealBackend>   (used by all tests)
  fn get_tier_boost(tier:&str) -> f32                              // backend.rs @866 (NOTE: moved from ~582)
  // CURRENT fusion (re-verified 2.0.0). THREE per-tier branches, all 100% vector, NO BM25:
  //   gate = 1.0 / (1.0 + (-20.0 * (similarity - 0.60)).exp());   recency = (-0.05 * delta_t).exp();
  //   EPISODE branch (@~1737):  w_imp=0.3  w_rec=0.3  importance_component = importance/10.0
  //   WIKI_NODE branch(@~1829): w_imp=0.4  w_rec=0.2  importance_component = utility/10.0   (CHANGED)
  //   WISDOM branch  (@~1878):  w_imp=0.5  w_rec=0.1  importance_component = utility/100.0  (CHANGED: no longer decay-immune)
  //   blended = similarity * ((w_imp*importance_component + w_rec*recency)/0.6) * get_tier_boost(<tier>) * gate;
  //   pass_threshold = (use_new_formula && is_vector) ? threshold*0.5 : threshold;   // vector queries over-fetch
  //   toggle: `use_new_formula` = is_sigmoid_gated_search_test || !is_running_in_test   (@~1440)
  // NOTE: scoring is 100% vector-derived. There is STILL NO lexical/BM25 channel today (Epic 4 valid).

db/schema.rs   (const INIT_SCHEMA: &str = "..."; one DEFINE block @ init. 2.0.0 = 187 lines)
  // IDIOM (2.0): EVERY definition uses `IF NOT EXISTS` (idempotent re-init). Follow this for all new fields:
  //   DEFINE FIELD IF NOT EXISTS <name> ON <table> TYPE <type>;
  // SurrealDB array syntax CONFIRMED in-tree (resolves prior SPEC-GAP):
  //   array<string>  e.g.  DEFINE FIELD IF NOT EXISTS labels ON entity TYPE array<string>;
  //   option<array<float>>  for embeddings.
  TABLE episode: title, content, source(DEFAULT ''), scope(DEFAULT 'general'), vault_path(DEFAULT ''),
                 processed_in_dream(bool DEFAULT false), embedding(option<array<float>>),
                 source_episode(option<record<episode>>), last_retrieved_at(option<string>),
                 utility(option<float>), importance(float DEFAULT 5.0), created_at(datetime DEFAULT time::now())
  TABLE relates_to TYPE RELATION IN episode|wiki_node|wisdom|handoff|entity|thought_node|belief_state
                   OUT (same set); fields: relation(option<string>), strength(option<float>),
                   created_at(datetime DEFAULT time::now())
  TABLE followed_by TYPE RELATION IN episode OUT episode; fields: duration(option<duration>), created_at
  TABLE mentions TYPE RELATION IN episode OUT entity; fields: created_at
  TABLE superseded_by TYPE RELATION IN wisdom OUT wisdom; fields: reason(option<string>), created_at
  TABLE entity: name(string UNIQUE), entity_type, summary, labels(array<string>), scope(DEFAULT 'general'), embedding
  // NEW tables in 2.0 (relevant to epics):
  TABLE metrics: target_id(record UNIQUE), utility_score(float DEFAULT 1.0), access_count(int DEFAULT 0),
                 last_accessed(datetime DEFAULT time::now())   // out-of-band utility/access tracking
  TABLE symbol_archive SCHEMALESS   // backs query_symbolic / symbolic graph (Epics 5 & 10)
  TABLE checkpoint_node SCHEMALESS; TABLE session_state SCHEMALESS
  TABLE chat_history: session_id, role, content, created_at   // auto-logged by call_mcp_tool wrapper
  TABLE wiki_node_history + EVENT wiki_node_update_history (versions wiki_node on UPDATE)
  TABLE belief_state: session_id, tasks_todo(array<string>), hypotheses_tested(array<string>),
                 confidence_score(float), uncertainty_areas(array<string>), updated_at  // auto-updated per tool call
  // STILL NO temporal validity (valid_from/valid_to) fields on ANY edge today. Epic 5 valid.

api.rs
  struct ApiState { backend:Arc<dyn StorageBackend>, auth_token:String, store:Arc<MarkdownStore>,
                    ignore_list:Arc<WatchIgnoreList>, dream_tx:Option<mpsc::Sender<()>> }
  fn create_router(state:Arc<ApiState>) -> Router            // register new routes HERE
  fn check_auth(headers:&HeaderMap, state:&ApiState) -> bool // header "X-Mythrax-Token"
  // existing handler pattern: async fn _handler(State(state), headers:HeaderMap, Json(payload)) -> Result<Json<Value>, StatusCode>

vault/watcher.rs
  async fn save_episode_bidirectional(episode:&EpisodeSave, backend:&Arc<dyn StorageBackend>,
                                      store:&Arc<MarkdownStore>, ignore_list:&WatchIgnoreList) -> Result<String>
  struct WatchIgnoreList; WatchIgnoreList::new() -> Self

store.rs
  struct MarkdownStore { vault_root:PathBuf }
  MarkdownStore::new<P:AsRef<Path>>(vault_root:P) -> Result<Self>
  fn write_file(&self, relative_path:&str, content:&str) -> Result<()>

cognitive/compactor.rs   (2.0.0 = 728 lines)
  struct Compactor; Compactor::new() -> Self   // NOTE: Compactor now holds an `llm` field (used during archive summarization)
  async fn compact_scope(&self, db:&dyn StorageBackend, store:&MarkdownStore, scope:&str,
                         embedder:Option<Arc<LocalEmbedder>>) -> Result<()>   // @75; calls prune_stale_memories + archive_decayed_episodes
  async fn compact_global(&self, ...) -> Result<()>   // @337 (NEW)
  async fn archive_decayed_episodes(&self, db:&dyn StorageBackend, store:&MarkdownStore) -> Result<()>   // PRIVATE @442
    // REWRITTEN in 2.0. Trigger: decayed_utility < decay_threshold*50.0   (profile key 'compaction.decay_threshold', default 0.15 => 7.5)
    //   decay_factor = calculate_decay_factor(now, last_retrieved_at) [pub fn @522]; decayed_utility = utility.unwrap_or(50.0)*decay_factor
    //   On trigger it: (1) std::fs::rename src -> vault/archive/<file>;  (2) LLM RAPTOR summary -> wiki/archive/raptor_summary_<8>.md + save_wiki_node;
    //                  (3) db.delete_by_vault_path(vp)   <-- THE DELETE Epic 3 converts to a demotion (compactor.rs @514)
  pub fn calculate_decay_factor(now, last_retrieved_at) -> f32   // @522

mcp_routes.rs   (NEW module in 2.0.0, ~2021 lines -- the MCP surface lives HERE, not in mcp.rs)
  pub async fn call_mcp_tool(state:&ApiState, name:&str, args:Value) -> Result<Value>   // @238 dispatch on 7 verb tools
    // wrapper side-effects after dispatch: auto-INSERT chat_history (assistant); auto-update belief_state.confidence_score
    //   (+0.02 ok / -0.05 err); journal_state() for manage_memory|manage_stm|manage_htr|manage_vault.
  // VERB TOOLS -> handlers (add new capability as an `action` arm + extend its inputSchema in tools[] @85):
  //   manage_memory  -> handle_manage_memory @366 -> handle_query_memory @379 (search|rules|nodes|root|query_symbolic)
  //                                               -> handle_record_memory @559 (save|feedback|thought)
  //   manage_htr     -> @688 (init|ideate|execute|backprop|merge|run)
  //   manage_stm     -> @892 (put|get|clear|handoff)
  //   manage_vault   -> @1010 (ingest_bulk|ingest_forge|save_forged_assets|verify|organize|reprocess|summarize|audit)
  //   manage_config  -> @1154 (get|set)
  //   pre_invocation_hook -> handle_pre_invocation_hook @1273  (read-path context injection; Self-RAG tiering)
  //   manage_file    -> @1772 (view|replace|multi_replace)  // virtual paging
  // Do NOT add a new top-level verb tool unless unavoidable.

Existing test files (follow these conventions exactly — re-verified UNCHANGED in 2.0.0):
  tests/test_sigmoid_gated_search.rs   tests/test_hybrid_hydration_hook.rs   tests/test_okf_watcher_sync.rs
  Convention: #[tokio::test]; SurrealBackend::new_in_memory().await?; backend.init().await?;
              construct EpisodeSave{...all 8 fields...}; backend.save_episode(&ep).await?;
              mutate fixtures via backend.db.query("UPDATE type::record('episode',$id) MERGE {...}").bind(("id",uuid)).await?.check()?;
              id format is "episode:<uuid>"; extract uuid via id.split(':').nth(1).unwrap().
  NEW reference tests in 2.0 to mirror per-epic:
    tests/test_schema_upgrades.rs        -- migration/new-field tests (mirror for Epics 3,8,9)
    tests/test_cycle_proof_traversal.rs  -- query_symbolic graph traversal (mirror for Epics 5,10)
    tests/test_pre_invocation_hook.rs    -- read-path hook (mirror for Epics 2,8 economics block)
    tests/test_compactor.rs              -- archive/decay behavior (mirror for Epic 3)
    tests/test_manage_file_paging_flow.rs / test_virtual_paging_editing_flow.rs -- MCP manage_file (mirror for Epic 10)
```

> **PROVENANCE & UPSTREAM VERIFICATION (checked 2026-06-26).** Every metric and constant below was verified against its true primary source, not just transcribed:
> - **nDCG@k** — canonical binary-gain Discounted Cumulative Gain normalized by ideal DCG (`DCG = Σ rel_i / log2(i+2)`). Matches the standard definition and the retrieval metric reported by the upstream LongMemEval paper ([LongMemEval, ICLR 2025, Wu et al.](https://github.com/xiaowu0162/LongMemEval); [arXiv 2410.10813](https://arxiv.org/html/2410.10813v1)). ✅ canonical.
> - **Okapi BM25** (`k1=1.5`, `b=0.75`, IDF `log((N-df+0.5)/(df+0.5)+1)`) — standard Okapi/Lucene-smoothed defaults ([Robertson & Zaragoza, as used in SWE-bench retrieval](https://openreview.net/pdf?id=VTF8yNQM66)). ✅ canonical.
> - **LongMemEval** — a real, peer-reviewed benchmark: 500 human-curated questions with official Recall@k / NDCG@k retrieval metrics and `has_answer` (turn) + `answer_session_ids` (session) labels ([upstream repo](https://github.com/xiaowu0162/LongMemEval); [arXiv 2410.10813](https://arxiv.org/html/2410.10813v1)). The `recall_any` / `recall_all` / `_turn_` granularity here map correctly to those labels. ✅ real & correctly attributed.
> - **CANONICAL DATASET SOURCE (use this, pinned).** Acquire the official 500-question set directly from the authors' published release on Hugging Face: dataset `xiaowu0162/longmemeval` (mirror of the original release; the `-cleaned` variant `xiaowu0162/longmemeval-cleaned` is byte-identical for `longmemeval_oracle.json`). Files: `longmemeval_s` (long-context), `longmemeval_m` (multi-session), `longmemeval_oracle` (gold evidence only). **Pin a specific revision (commit SHA) and record a SHA-256 of each downloaded file in the benchmark output** so results are reproducible and auditable. Do NOT use any pre-split or repackaged copy from a reference-implementation repo as the data source.
> - **DATASET INTEGRITY — audited 2026-06-26 (this session).** I downloaded the reference-impl convenience split (`lme_split_50_450.json`) and cross-checked all 500 of its IDs against the official `longmemeval_oracle` set: **500/500 IDs are genuine official LongMemEval questions, 0 fabricated, 0 modified, 0 missing**, and the per-question-type distribution exactly reproduces the official full-set distribution (temporal-reasoning 133, multi-session 133, knowledge-update 78, single-session-user 70, single-session-assistant 56, single-session-preference 30). The only non-canonical element is the *partitioning act* (`random.Random(42)` → 50 dev / 450 held-out), not the data. ✅ data is real and unmanipulated.
> - **⚠️ PUBLISHABILITY RULE — the 50/450 split is NOT canonical LongMemEval.** `dev_size=50, seed=42` is a reference-impl convenience partition, not part of the published LongMemEval protocol. **For publishable results, Epic 1's DEFAULT and REPORTED run MUST be the full official 500-question set, scored per the upstream protocol** (Recall@k / NDCG@k over all 500). Any 50/450 numbers are an internal-only fast regression gate and must NEVER be published, labeled "LongMemEval baseline," or compared to the official leaderboard. Every published number must name the dataset id + revision + checksum + k values + which split (always "full 500" for publication).
> - **Scope note (what you can honestly claim):** LongMemEval's *headline* metric is end-to-end QA accuracy (LLM-as-judge). Epic 1 measures **retrieval** (Recall@k / NDCG@k), the intermediate metric. This is correct and publishable **as a retrieval result** — but it must be labeled "LongMemEval *retrieval* (Recall@k / NDCG@k)", never presented as LongMemEval QA accuracy. To claim end-to-end QA accuracy you would additionally need the LLM-judge QA stage, which is out of scope for Epic 1.
> - **SWE-bench Verified** (Epic 11B) — the official `princeton-nlp/SWE-bench_Verified` dataset (500 human-verified issues); primary metric "% Resolved" ([dataset](https://huggingface.co/datasets/princeton-nlp/SWE-bench_Verified); [leaderboard](https://www.swebench.com)). ✅ verified.

```
# REFERENCE BEHAVIORS & PARAMETERS (self-contained — implement natively in Rust; no external code to read)

## Retrieval metrics (Epic 1)  [verified canonical — see PROVENANCE block above]
  evaluate_retrieval(rankings, correct_ids, corpus_ids, k) -> (recall_any, recall_all, ndcg)
     top_k_ids  = { corpus_ids[idx] for idx in rankings[..k] }            # set of top-k retrieved ids
     recall_any = 1.0 if ANY correct_id in top_k_ids else 0.0
     recall_all = 1.0 if ALL correct_ids in top_k_ids else 0.0
     # nDCG@k (binary relevance):
     #   rel_i = 1.0 if corpus_ids[rankings[i]] in correct_ids else 0.0   for i in 0..k
     #   dcg   = sum( rel_i / log2(i + 2) )
     #   idcg  = dcg( sort(rel, descending) );  ndcg = dcg/idcg (0.0 if idcg==0)
     ndcg       = binary-gain nDCG@k as above
  session_id_from_corpus_id(corpus_id): strip trailing "_turn_N" suffix (rsplit on "_turn_")
  PUBLISHED EVAL (default): score over the FULL official 500-question LongMemEval set (canonical source, pinned id+rev+sha256 — see PROVENANCE block). Report Recall@k / nDCG@k per upstream protocol + per-question-type breakdown. This is the only mode whose numbers may be published.
  INTERNAL REGRESSION GATE (optional, never published): dev_size=50, seed=42 -> { dev:[50], held_out:[450] } reference-impl convenience partition, for fast CI no-regression checks only. Must be labeled "internal split, not LongMemEval" in any output and never compared to the official leaderboard (see PROVENANCE block).

## Hybrid lexical+vector fusion & boosts (Epic 4)
  hybrid score = 0.6*vector + 0.4*bm25_norm   (vector_weight=0.6, bm25_weight=0.4)
  BM25 Okapi: k1=1.5, b=0.75, IDF=log((N-df+0.5)/(df+0.5)+1), min-max normalized within the candidate set
  rank-position boost ladder: [0.40, 0.25, 0.15, 0.08, 0.04]; effective_dist = clamp(dist - boost, 0.0, 2.0)
  candidate over-fetch = n_results*3; MAX_HYDRATION_CHARS = 10000
  person-name match => -40% distance; quoted-phrase match => -60% distance

## Bitemporal edges (Epic 5)
  add_triple(subject, predicate, obj, valid_from?, valid_to?, confidence=1.0): reject when valid_to < valid_from
  invalidate(subject, predicate, obj, ended?): set valid_to instead of deleting the edge
  query as_of(name, as_of?, direction?): filter (valid_from IS NULL OR valid_from<=T) AND (valid_to IS NULL OR valid_to>=T)
  date-only normalization: valid_from -> T00:00:00Z, valid_to -> T23:59:59Z; "current" := valid_to IS NULL

## Hook shell helpers (Epics 2 & 6)
  sanitize_session_id(x): strip chars not in [a-zA-Z0-9_-]; empty -> "unknown"
  normalize_transcript_path(p): replace "\\" -> "/", strip [\x00\r\n] (preserve Windows drive letters)
  count_human_messages(path): count JSONL lines where message.role=="user" and "<command-message>" not in content
  SAVE_INTERVAL = 15 (human messages)
```

---

## EPIC 1 — Retrieval benchmark harness (DO FIRST; gate for all others)

**Spec:** A reproducible harness that ingests the **official LongMemEval dataset** into a fresh in-memory `SurrealBackend`, queries it, and computes `recall_any@k`, `recall_all@k`, and `nDCG@k` against gold chunk IDs — using the metric definitions in REFERENCE BEHAVIORS above (verified canonical — see PROVENANCE block). **DEFAULT MODE = the full official 500-question set** acquired from the canonical primary source (pinned dataset id + revision + SHA-256, per PROVENANCE block) and scored per the upstream protocol; these are the **only publishable** numbers, and every reported result must carry its dataset id/revision/checksum, k values, and the label "LongMemEval *retrieval* (Recall@k / NDCG@k)" (never "QA accuracy"). A second, **optional internal-only** mode runs the seed=42 50/450 convenience partition purely as a fast CI no-regression gate — its numbers are never published, never called "LongMemEval baseline," and never compared to the official leaderboard. No production code changes; this epic only adds a bench crate/module, the dataset-acquisition+integrity step, and the baseline record.

**Why first:** Epics 4 and 5 are retrieval-quality bets. Without a held-out gate you cannot tell if a change helps or regresses. This epic produces the BASELINE number that all later epics must not regress.

### SYMBOL VERIFICATION
```
# confirm these resolve before coding:
ast-grep -p 'async fn search($$$)' mythrax-core/src/db/backend.rs
ast-grep -p 'pub async fn new_in_memory($$$)' mythrax-core/src/db/backend.rs   # or grep "fn new_in_memory"
grep -n "pub struct SearchResult" mythrax-core/src/contracts.rs
# the metric definition is fully specified in REFERENCE BEHAVIORS above — no external file to consult
```

### TESTS (write first) — `mythrax-core/tests/test_bench_metrics.rs`
```rust
use mythrax_core::bench::metrics::{evaluate_retrieval, RetrievalScore, ndcg};

#[test]
fn recall_any_true_when_one_gold_in_topk() {
    // corpus ids c0..c4, rankings put c3 (a gold) at rank 2 (within k=5)
    let corpus = vec!["c0","c1","c2","c3","c4"].iter().map(|s|s.to_string()).collect::<Vec<_>>();
    let rankings = vec![0usize,3,1,2,4];        // index order into corpus
    let gold = vec!["c3".to_string(), "c9_missing".to_string()];
    let s = evaluate_retrieval(&rankings, &gold, &corpus, 5);
    assert_eq!(s.recall_any, 1.0);              // at least one gold present
    assert_eq!(s.recall_all, 0.0);              // not ALL golds present (c9_missing absent)
}

#[test]
fn recall_all_requires_every_gold_in_topk() {
    let corpus = vec!["c0","c1","c2","c3","c4"].iter().map(|s|s.to_string()).collect::<Vec<_>>();
    let rankings = vec![3usize,1,0,2,4];
    let gold = vec!["c3".to_string(), "c1".to_string()];
    let s = evaluate_retrieval(&rankings, &gold, &corpus, 5);
    assert_eq!(s.recall_all, 1.0);
}

#[test]
fn k_cutoff_excludes_gold_beyond_k() {
    let corpus = vec!["c0","c1","c2","c3","c4"].iter().map(|s|s.to_string()).collect::<Vec<_>>();
    let rankings = vec![0usize,1,2,4,3];        // gold c3 is at rank 5 (index 4) -> outside k=4
    let gold = vec!["c3".to_string()];
    assert_eq!(evaluate_retrieval(&rankings,&gold,&corpus,4).recall_any, 0.0);
    assert_eq!(evaluate_retrieval(&rankings,&gold,&corpus,5).recall_any, 1.0);
}

#[test]
fn ndcg_rewards_higher_rank() {
    let corpus = vec!["c0","c1"].iter().map(|s|s.to_string()).collect::<Vec<_>>();
    let gold = vec!["c1".to_string()];
    let high = ndcg(&vec![1usize,0], &gold, &corpus, 2);   // gold first
    let low  = ndcg(&vec![0usize,1], &gold, &corpus, 2);   // gold second
    assert!(high > low);
}
```
Integration test `mythrax-core/tests/test_bench_e2e_smoke.rs`:
```rust
// Ingest 3 episodes via save_episode, query, assert harness returns a populated
// SearchResponse and the metric runner produces finite scores. Uses SurrealBackend::new_in_memory().
```

### IMPLEMENTATION
- New module `mythrax-core/src/bench/mod.rs` + `bench/metrics.rs` (compile under `#[cfg(any(test, feature="bench"))]` or a small `bench` feature so it never ships in the daemon binary).
- `pub struct RetrievalScore { pub recall_any:f32, pub recall_all:f32, pub ndcg:f32 }`
- `pub fn evaluate_retrieval(rankings:&[usize], correct_ids:&[String], corpus_ids:&[String], k:usize) -> RetrievalScore` — implement the metric exactly per REFERENCE BEHAVIORS → Retrieval metrics (recall_any/recall_all/binary-gain nDCG@k), written natively in Rust. `rankings[i]` indexes into `corpus_ids`. Build `top_k = rankings[..k].map(|i| corpus_ids[i])`.
- `pub fn ndcg(rankings, correct_ids, corpus_ids, k) -> f32` — standard binary-gain nDCG@k.
- `pub fn session_id_from_corpus_id(id:&str) -> &str` — strip `_turn_<n>` suffix (per REFERENCE BEHAVIORS) so session vs. turn granularity matches.
- New `bench/runner.rs` (binary `cargo run --bin bench -- --split full500 | internal-gate`): loads the dataset JSON, ingests via `backend.save_episode` / `save_episode_bidirectional`, queries via `backend.search(query, scope, false, k, 0, 0.0, None, false, true, true)`, maps returned `SearchResult.id` → corpus index, computes scores, writes **JSONL** per-question output (one record per question: id, type, ranked ids, per-metric scores), and prints aggregate + per-question-type breakdown. **The run header/manifest MUST embed: dataset id, pinned revision (commit SHA), per-file SHA-256, split mode, k values, and Mythrax git commit** — so any published number is fully reproducible. `--split full500` is the **default and the only publishable mode**; `--split internal-gate` is CI-only and stamps every record `"published": false, "note": "internal split, not LongMemEval"`.
- **Dataset acquisition + integrity (DEFAULT full500).** Acquire the official LongMemEval files from the canonical primary source pinned in the PROVENANCE block (HF dataset `xiaowu0162/longmemeval` at a fixed revision), place them under `mythrax-core/bench_data/official/`, and on load **verify the recorded SHA-256 and assert exactly 500 unique `question_id`s** before scoring. If the official files are absent, or any checksum/count check fails, emit `SPEC-GAP: official LongMemEval dataset missing or integrity check failed` and stop — do NOT substitute a derived/pre-split copy and do NOT fetch from any reference-impl repo. For the optional `internal-gate` mode only, the seed=42 50/450 partition may be regenerated locally from the verified official 500 (tune on `dev`, evaluate on `held_out` once); it is never published.

### ACCEPTANCE
- [ ] All `test_bench_metrics.rs` cases pass; `evaluate_retrieval` produces the expected scores on a hand-computed fixture (verify one case by hand against the formula in REFERENCE BEHAVIORS).
- [ ] `bench` module/feature does NOT link into the release daemon binary.
- [ ] Runner emits JSONL + aggregate `recall_any@5`, `recall_all@5`, `nDCG@10`, plus per-question-type `R@10`.
- [ ] **Dataset integrity gate:** default `full500` run loads the official files, verifies each recorded SHA-256, and asserts exactly 500 unique `question_id`s before scoring; mismatch → `SPEC-GAP` + stop (no fallback to a derived copy).
- [ ] **Reproducibility manifest:** every output (JSONL header + BASELINE.md) records dataset id, pinned revision SHA, per-file SHA-256, split mode, k values, and Mythrax commit; `internal-gate` records are stamped `"published": false`.
- [ ] **PUBLISHED BASELINE RECORDED:** run the runner in **`full500`** mode against current `main` and commit the official-set numbers to `mythrax-core/bench_data/BASELINE.md`, labeled "LongMemEval *retrieval* (Recall@k / NDCG@k), full 500". Every later epic re-runs `full500` and must not regress `recall_any@5`. (An optional `internal-gate` run may be recorded separately for fast CI, clearly marked not-for-publication.)

---

## EPIC 2 — PreCompact emergency-save hook (capture before context loss)

**Spec:** Add a synchronous capture path that mines a host transcript JSONL into Mythrax memory *before* the host compacts context, persisting raw user turns AND raw tool output verbatim. This is a NEW write-path capability; Mythrax's existing `pre_invocation_hook` is read-path (injection) and is unaffected.

### SYMBOL VERIFICATION
```
grep -n "fn create_router" mythrax-core/src/api.rs
grep -n "fn check_auth" mythrax-core/src/api.rs
grep -n "pub async fn save_episode_bidirectional" mythrax-core/src/vault/watcher.rs
```

### TESTS (write first) — `mythrax-core/tests/test_precompact_hook.rs`
```rust
// Pure sanitizer unit tests (per REFERENCE BEHAVIORS → Hook shell helpers):
#[test] fn sanitize_session_id_strips_unsafe() {
    assert_eq!(mythrax_core::hooks::shell::sanitize_session_id("a/b c.d"), "abcd");
    assert_eq!(mythrax_core::hooks::shell::sanitize_session_id(""), "unknown");
}
#[test] fn normalize_path_preserves_windows_drive() {
    let p = mythrax_core::hooks::shell::normalize_transcript_path("C:\\Users\\me\\s.jsonl");
    assert_eq!(p, "C:/Users/me/s.jsonl");                 // backslashes->slashes, colon kept
}
#[test] fn count_human_messages_skips_command_messages() {
    // write a temp .jsonl: 2 user msgs (one containing "<command-message>"), 1 assistant
    // assert count == 1
}
```
Integration `tests/test_precompact_ingest.rs`:
```rust
#[tokio::test]
async fn precompact_persists_raw_tool_output() -> anyhow::Result<()> {
    // 1. build in-memory backend + MarkdownStore(tempdir) + WatchIgnoreList
    // 2. write a temp transcript.jsonl containing a user turn AND a tool_result with raw output text "RAW_TOOL_PAYLOAD_XYZ"
    // 3. call hooks::precompact::mine_transcript("sess1", path, &backend, &store, &ignore)
    // 4. assert returned count >= 2
    // 5. backend.search("RAW_TOOL_PAYLOAD_XYZ", Some("general"), false, 5, 0, 0.0, None, false, true, true)
    //    -> at least one result whose content contains the raw payload (proves verbatim tool output captured)
    Ok(())
}
```
Route auth test (extend `api.rs` test conventions): `POST /v1/hooks/precompact` without token ⇒ 401; with token + valid body ⇒ 200 and `{"status":"success","episodes_saved":N}`.

### IMPLEMENTATION
- New `mythrax-core/src/hooks/mod.rs`, `hooks/shell.rs`, `hooks/precompact.rs`.
- `hooks/shell.rs` — pure helpers: `sanitize_session_id(&str)->String`, `normalize_transcript_path(&str)->String`, `count_human_messages(path:&str)->usize`. Implement exactly per REFERENCE BEHAVIORS → Hook shell helpers.
- `hooks/precompact.rs::mine_transcript(session:&str, transcript_path:&str, backend:&Arc<dyn StorageBackend>, store:&Arc<MarkdownStore>, ignore:&WatchIgnoreList) -> Result<usize>`: read JSONL line-by-line; for each user turn and each tool-result turn, build an `EpisodeSave` (all 8 fields; `session_id: Some(session.into())`, `scope: Some("general")`, `entities: vec![]`, `vault_path: None`) with **content = the raw text verbatim** (do NOT summarize), funnel through `save_episode_bidirectional`; return episodes saved.
- `api.rs`: add `struct PrecompactPayload { session_id:String, transcript_path:String }`; add handler matching the existing pattern (auth via `check_auth`, sanitize inputs, call `mine_transcript`); register `.route("/v1/hooks/precompact", post(precompact_handler))` inside `create_router`.
- **2.0 reuse:** after the mine completes, call `backend.journal_state(&store.vault_root, Some(session))` to mirror the dual-durability behavior the MCP wrapper already applies after mutating tools (do not re-implement journaling). The new emergency-save fits the existing write-then-journal contract.
- Shell shim `hooks/mythrax_precompact_hook.sh` (bash 3.2 safe — no `mapfile`; `umask 077` on any state file; `head -c` byte cap on logged payloads): read host hook JSON from stdin, `curl -s -H "X-Mythrax-Token: $MYTHRAX_TOKEN" -d @- http://127.0.0.1:8090/v1/hooks/precompact`.

### ACCEPTANCE
- [ ] Sanitizer unit tests pass; behavior matches REFERENCE BEHAVIORS → Hook shell helpers on the specified inputs.
- [ ] Integration test proves **raw tool output** is retrievable post-mine (the core value).
- [ ] Route returns 401 unauth / 200 auth; `mine_transcript` is synchronous (awaited before handler returns).
- [ ] No change to `pre_invocation_hook` behavior (regression-check its existing test still passes).
- [ ] Epic 1 baseline not regressed.

---

## EPIC 3 — Verbatim-as-bottom-tier (stop deleting embedded originals)

**Spec (revised for 2.0.0):** Decayed episodes must remain embedded and retrievable as a low-ranked floor instead of being removed from the index. In 2.0 the decay trigger is `decayed_utility < decay_threshold*50.0` (profile key `compaction.decay_threshold`, default 0.15 ⇒ effective 7.5), where `decayed_utility = utility.unwrap_or(50.0) * calculate_decay_factor(now, last_retrieved_at)`. On trigger, `archive_decayed_episodes` already (a) moves the file to `vault/archive/`, (b) writes a RAPTOR summary `wiki_node`, then (c) calls `delete_by_vault_path` — step (c) removes the searchable original. Change (c) to a *demotion*, not a *delete*. The physical-archive + RAPTOR steps stay.

### SYMBOL VERIFICATION
```
grep -n "async fn archive_decayed_episodes" mythrax-core/src/cognitive/compactor.rs   # expect @442
grep -n "delete_by_vault_path" mythrax-core/src/cognitive/compactor.rs   # expect the call at ~514
grep -n "decay_threshold" mythrax-core/src/cognitive/compactor.rs        # confirm trigger + profile key
grep -n "fn calculate_decay_factor" mythrax-core/src/cognitive/compactor.rs  # expect pub fn @522
grep -n "DEFINE FIELD IF NOT EXISTS archived ON episode" mythrax-core/src/db/schema.rs  # expect EMPTY (to be added)
```

### TESTS (write first) — `mythrax-core/tests/test_verbatim_floor.rs`
```rust
#[tokio::test]
async fn decayed_episode_still_retrievable_but_demoted() -> anyhow::Result<()> {
    // 1. save two episodes; set ep_low.utility = 1.0 (decayed), ep_hi.utility = 80.0, both similar content
    //    via backend.db.query("UPDATE type::record('episode',$id) MERGE { utility: 1.0 }").bind(...).check()
    // 2. run Compactor::new().compact_scope(&backend, &store, "general", None).await?  (invokes archive_decayed)
    // 3. ASSERT ep_low is STILL returned by backend.search(query,...threshold 0.0...) -> NOT deleted
    // 4. ASSERT ep_low ranks below ep_hi (decayed = floor, not gone)
    Ok(())
}
#[tokio::test]
async fn raptor_summary_is_additive_not_replacement() -> anyhow::Result<()> {
    // after compaction, BOTH the raptor wiki_node AND the original episode resolve.
    Ok(())
}
```

### IMPLEMENTATION
- `schema.rs`: add `DEFINE FIELD IF NOT EXISTS archived ON episode TYPE bool DEFAULT false;` (idempotent; safe on existing DBs).
- `compactor.rs::archive_decayed_episodes`: REMOVE the `db.delete_by_vault_path(vp).await?` call (now at ~line 514). Instead set `archived = true` and clamp `utility`/`importance` low via a SurrealQL `UPDATE type::record('episode',$id) MERGE {...}` so the HNSW vector + record persist. Keep the file-move-to-`vault/archive/` and the RAPTOR summary `save_wiki_node` steps unchanged.
- **2.0 note:** because the physical `.md` is moved out of the watched vault, the bidirectional file is gone but the DB record (with embedding) is what now matters — demotion keeps it searchable. Optionally consider `reinforce_episode` as the inverse signal (a retrieved-while-archived episode could be un-archived) — out of scope here, note only.
- Optional (gate on Epic 1 numbers): in the episode branch of `backend.rs::search`, apply a small multiplicative penalty when `archived == true` so decayed originals rank last but remain eligible. Wire decayed originals into the `allow_downward` tier already plumbed through `search`.
- Bounded verbatim hydration (`MAX_HYDRATION_CHARS=10000`, per REFERENCE BEHAVIORS): when hydrating a full result, cap injected verbatim content at 10k chars.

### ACCEPTANCE
- [ ] Decayed episode is retrievable after compaction (no silent recall cliff).
- [ ] RAPTOR summary coexists with original (additive).
- [ ] `delete_by_vault_path` is no longer called from the decay path (grep proves it).
- [ ] Epic 1 `recall_all@5` improves or holds on held-out (verbatim floor should not regress; often lifts recall_all).

---

## EPIC 4 — Lexical (BM25) channel + cheap retrieval boosts

**Spec:** Augment Mythrax's vector-only fusion with a BM25 lexical channel and four cheap distance-reduction boosts (person-name, exact-quote, temporal-proximity, keyword-overlap), each independently feature-flagged and ablated against Epic 1's held-out split. Use the weights in REFERENCE BEHAVIORS as priors. Boosts are SIGNALS, never GATES — verbatim/vector retrieval remains the floor.

### SYMBOL VERIFICATION
```
grep -n "let blended_score" mythrax-core/src/db/backend.rs          # the 3 fusion branches
grep -n "fn get_tier_boost" mythrax-core/src/db/backend.rs
# confirm current scoring has no bm25:
grep -ni "bm25\|lexical" mythrax-core/src/db/backend.rs              # expect EMPTY
```

### TESTS (write first) — `mythrax-core/tests/test_boosts.rs`
```rust
use mythrax_core::retrieval::boosts::{apply_boosts, BoostSignals, BoostWeights};

#[test] fn boosts_clamp_to_zero_two_range() {
    let w = BoostWeights::default();
    let d = apply_boosts(0.10, &BoostSignals{ person_name:true, exact_quote:true, ..Default::default() }, &w);
    assert!(d >= 0.0 && d <= 2.0);                       // effective_dist = max(0, min(2, dist - boost))
}
#[test] fn person_name_reduces_distance_about_40pct() {
    let w = BoostWeights::default();
    let base = 1.0;
    let boosted = apply_boosts(base, &BoostSignals{ person_name:true, ..Default::default() }, &w);
    assert!(boosted < base);
    assert!((boosted - 0.60).abs() < 1e-3);              // -40% (per REFERENCE BEHAVIORS)
}
#[test] fn quoted_phrase_reduces_distance_about_60pct() {
    let w = BoostWeights::default();
    let boosted = apply_boosts(1.0, &BoostSignals{ exact_quote:true, ..Default::default() }, &w);
    assert!((boosted - 0.40).abs() < 1e-3);              // -60% (per REFERENCE BEHAVIORS)
}
#[test] fn no_signals_is_identity() {
    assert_eq!(apply_boosts(0.73, &BoostSignals::default(), &BoostWeights::default()), 0.73);
}
```
BM25 unit tests `tests/test_bm25.rs`: build a tiny corpus, assert Okapi BM25 (k1=1.5, b=0.75) ranks the doc containing the query term highest; assert min-max normalization to [0,1] within candidate set.
Integration `tests/test_hybrid_fusion.rs`: an episode that matches lexically but weakly on vector should rank higher with the BM25 channel ON than OFF (gate the channel behind a flag and test both states).

### IMPLEMENTATION
- New `mythrax-core/src/retrieval/mod.rs`, `retrieval/bm25.rs`, `retrieval/boosts.rs`.
- `boosts.rs`: `struct BoostSignals { person_name:bool, exact_quote:bool, temporal_proximity:f32, keyword_overlap:f32 }` (derive `Default`); `struct BoostWeights { person_name:f32 /*0.40*/, exact_quote:f32 /*0.60*/, ... }`; `fn apply_boosts(base_dist:f32, sig:&BoostSignals, w:&BoostWeights) -> f32` returning `(base_dist - total_boost).clamp(0.0, 2.0)`. Person-name boost = `0.40*base` (−40%), quote = `0.60*base` (−60%) — express as fractional distance reductions per REFERENCE BEHAVIORS; keep pure + unit-testable.
- `bm25.rs`: in-process Okapi BM25 over candidate chunks (`k1=1.5`, `b=0.75`, IDF `log((N-df+0.5)/(df+0.5)+1)`), min-max normalized within candidate set. (If you prefer SurrealDB full-text search, gate it behind the same flag and keep the in-process impl as the test oracle.)
- `backend.rs::search`: over-fetch `limit*3` candidates (per REFERENCE BEHAVIORS); compute `bm25_norm`; fuse `fused_vec_sim = 0.6*similarity + 0.4*bm25_norm` BEHIND a feature flag; derive `BoostSignals` from query↔candidate and apply via `apply_boosts` on the distance form. Preserve the existing sigmoid gate and tier boost. Each boost behind its own flag/config so it can be toggled for ablation.
- Config: surface flags via `manage_config` / env so the bench runner can toggle per ablation.

### ACCEPTANCE
- [ ] `apply_boosts` + BM25 unit tests pass; boosts are pure and clamped to [0,2].
- [ ] Each boost individually toggleable; default-off until proven.
- [ ] **Ablation table reproduced:** for each boost, run Epic 1 held-out and record Δ`recall_any@5`. Keep a boost ONLY if Δ ≥ 0. Commit the table to `bench_data/ABLATION.md`.
- [ ] Verbatim/vector path still returns results when all boosts off (no gating).

---

## EPIC 5 — Temporal validity windows on edges (`as_of` queries)

**Spec:** Add bitemporal validity to graph edges so Mythrax can answer "what was true about X at time T". Implement `valid_from`/`valid_to` validity *semantics* (per REFERENCE BEHAVIORS → Bitemporal edges) natively on SurrealDB's `relates_to` edge, with `invalidate` (close, don't delete) and an `as_of` query filter.

### SYMBOL VERIFICATION
```
grep -n "DEFINE TABLE IF NOT EXISTS relates_to" mythrax-core/src/db/schema.rs
grep -n "async fn relate_nodes" mythrax-core/src/db/backend.rs
grep -ni "valid_from\|valid_to" mythrax-core/src/db/schema.rs       # expect EMPTY (to be added)
# bitemporal edge semantics are fully specified in REFERENCE BEHAVIORS above — no external file to consult
```

### TESTS (write first) — `mythrax-core/tests/test_temporal_edges.rs`
```rust
#[tokio::test]
async fn as_of_returns_only_facts_valid_then() -> anyhow::Result<()> {
    // 1. save two episodes A,B; relate A->B with valid_from=2025-01-01, valid_to=2025-06-01
    //    (new method relate_nodes_temporal or relate_nodes with optional window)
    // 2. query edges as_of=2025-03-01 -> edge present
    // 3. query edges as_of=2025-09-01 -> edge ABSENT (outside window)
    Ok(())
}
#[tokio::test]
async fn invalidate_closes_not_deletes() -> anyhow::Result<()> {
    // relate A->B open-ended (valid_to=None); invalidate sets valid_to;
    // edge row still EXISTS (history preserved); current(valid_to IS NULL) == false afterward.
    Ok(())
}
#[tokio::test]
async fn reject_inverted_interval() -> anyhow::Result<()> {
    // valid_to < valid_from -> Err (per REFERENCE BEHAVIORS; original Rust)
    Ok(())
}
```

### IMPLEMENTATION
- `schema.rs`: extend `relates_to` (idempotent `DEFINE FIELD IF NOT EXISTS`):
  ```
  DEFINE FIELD IF NOT EXISTS valid_from ON relates_to TYPE option<datetime>;
  DEFINE FIELD IF NOT EXISTS valid_to   ON relates_to TYPE option<datetime>;
  DEFINE FIELD IF NOT EXISTS confidence ON relates_to TYPE float DEFAULT 1.0;
  DEFINE INDEX IF NOT EXISTS idx_relates_valid ON relates_to FIELDS valid_from, valid_to;
  ```
- `backend.rs`: add `relate_nodes_temporal(&self, from:&str, to:&str, valid_from:Option<&str>, valid_to:Option<&str>, confidence:f32) -> Result<()>` (or extend `relate_nodes` with optional window) with the inverted-interval guard; add `invalidate_edge(&self, from:&str, to:&str, ended:Option<&str>)` that `UPDATE`s `valid_to` instead of deleting; add `query_edges_as_of(&self, node:&str, as_of:&str)` using filter `(valid_from = NONE OR valid_from <= $as_of) AND (valid_to = NONE OR valid_to >= $as_of)`. Add new trait methods to `StorageBackend`.
- On write, stamp `valid_from = created_at`. On supersession (reuse the existing `superseded_by`/wisdom supersession trigger), close the prior edge's `valid_to`.
- Optional: thread an `as_of: Option<&str>` param into `search`/`search_handler` for point-in-time retrieval.
- **2.0 reuse — do not reinvent traversal:** Mythrax 2.0 already ships `query_symbolic(node_id, relation, max_depth) -> Vec<String>` (backend.rs @158) backed by the `symbol_archive` table and exercised by `tests/test_cycle_proof_traversal.rs`. Implement `query_edges_as_of` as a temporal FILTER layered on the same `relates_to` traversal `query_symbolic` already performs (add an optional `as_of` arg to `query_symbolic`, or a sibling that shares its cycle-safe walk), NOT as a brand-new graph walker. Mirror `tests/test_cycle_proof_traversal.rs` conventions.

### ACCEPTANCE
- [ ] `as_of` filter returns only facts valid at T; out-of-window edges excluded.
- [ ] Temporal filter composes with the existing `query_symbolic` cycle-safe traversal (no second graph walker introduced).
- [ ] `invalidate` preserves the row (history), flips `current`.
- [ ] Inverted interval rejected.
- [ ] Schema change idempotent on an existing DB (run init twice; no error).
- [ ] Validate on LongMemEval temporal-reasoning question types via Epic 1's per-qtype breakdown — temporal R@10 improves or holds.

---

## EPIC 6 — Block-and-save Stop hook (cadence capture)

**Spec:** Add a `/v1/hooks/stop` endpoint that, every `SAVE_INTERVAL = 15` human messages, mines the transcript (reusing Epic 2's pipeline) and returns a block decision so the host briefly pauses while memory persists, guarded against re-entry via `stop_hook_active`.

### SYMBOL VERIFICATION
```
grep -n "fn create_router" mythrax-core/src/api.rs
grep -n "mine_transcript" mythrax-core/src/hooks/precompact.rs   # must exist from Epic 2
# count_human_messages + SAVE_INTERVAL=15 are specified in REFERENCE BEHAVIORS above
```

### TESTS (write first) — `mythrax-core/tests/test_stop_hook.rs`
```rust
#[test] fn cadence_triggers_every_15_human_messages() {
    // helper should_save(prev_count, new_count) -> bool; crossing a multiple of 15 => true
    assert!(mythrax_core::hooks::stop::should_save(14, 15));
    assert!(!mythrax_core::hooks::stop::should_save(15, 16));
    assert!(mythrax_core::hooks::stop::should_save(29, 30));
}
#[test] fn stop_hook_active_prevents_reentry() {
    // when stop_hook_active == true, handler must no-op (return without re-mining)
}
```
Route auth test: `POST /v1/hooks/stop` 401 unauth / 200 auth.

### IMPLEMENTATION
- `hooks/stop.rs`: `fn should_save(prev:usize, new:usize) -> bool` (crossed a 15-boundary); `mine_if_due(...)` reuses `precompact::mine_transcript`; honor `stop_hook_active` (no-op if set).
- `api.rs`: `struct StopPayload { session_id:String, stop_hook_active:bool, transcript_path:String }`; handler + `.route("/v1/hooks/stop", post(stop_handler))`.
- Shell shim `hooks/mythrax_save_hook.sh`: returns block decision to host, curls endpoint; opt-out via `MYTHRAX_HOOKS_AUTO_SAVE=false`.

### ACCEPTANCE
- [ ] Cadence + re-entry guard unit tests pass.
- [ ] Endpoint auth-gated; reuses Epic 2 mining (no duplicate logic).
- [ ] Opt-out env var respected.

---

## EPIC 7 — Harness-agnostic hook adapters

**Spec:** Thin per-host adapters (Claude Code, Codex, Cursor, Antigravity/Gemini) that normalize each host's hook payload to Mythrax's canonical shape and call `/v1/hooks/*`. All real logic stays server-side; adapters are pure translation.

### SYMBOL VERIFICATION
```
ls mythrax-core/.. /hooks/   # confirm Epic 2 & 6 shims exist
# normalize_transcript_path semantics are specified in REFERENCE BEHAVIORS above
```

### TESTS (write first) — `mythrax-core/tests/test_hook_adapters.rs`
```rust
#[test] fn claude_code_payload_maps_to_canonical() {
    // given a sample Claude Code Stop payload JSON, adapter yields (session_id, stop_hook_active, transcript_path)
    // with Windows path normalized and session_id sanitized.
}
#[test] fn codex_payload_maps_to_canonical() { /* different field names -> same canonical tuple */ }
```

### IMPLEMENTATION
- One adapter per host: parse host JSON → canonical tuple → curl the right `/v1/hooks/*`. Keep logic in the Rust daemon; adapters only translate.
- Portability hardening across all shims: bash 3.2 (no `mapfile`), `umask 077` state files, `head -c` caps, path-traversal-safe state filenames, parse sentinel `__MYTHRAX_PARSE_OK__`.

### ACCEPTANCE
- [ ] Each adapter maps its host's sample payload to the canonical tuple (golden-file tests).
- [ ] No business logic in adapters (review: translation only).
- [ ] Shims pass shellcheck and run under macOS bash 3.2.

---

---

# APPENDIX A — Coding-agent memory enhancements (Epics 8–11)

These four capabilities are common to mature coding-agent memory layers and address gaps in Mythrax's current design. They are folded in as Epics 8–11. Same rules apply: tests first, AST-anchored, implemented natively in Rust, `SPEC-GAP:` + halt on any missing symbol. Everything you need is specified here; there is no external codebase to read.

## REFERENCE BEHAVIORS & PARAMETERS — coding-agent memory (self-contained)
```
## Structured memory item (Epic 9)
  MemoryItem fields of interest: kind ('observation'|'summary'|'prompt'|'manual'),
    type:string, title, subtitle, narrative, facts:string[], concepts:string[],
    files_read:string[], files_modified:string[], metadata

## Token economics (Epic 8)
  observation_tokens(obs) = ceil( len(title)+len(subtitle)+len(narrative)+len(JSON(facts)) / CHARS_PER_TOKEN )
  CHARS_PER_TOKEN = 4 (estimate)
  token_economics: savings = sum(discovery_tokens) - sum(read_tokens); savings_percent accordingly

## Concept-filtered retrieval (Epic 9)
  pre-filter: WHERE type IN (...) AND (any requested concept present in concepts[])
              ORDER BY created_at DESC LIMIT total_count

## 3-layer progressive disclosure (Epic 10)
  layer 1 search  -> compact index (id + title + subtitle only, ~50–100 tokens/result)
  layer 2 timeline-> chronological neighbors of an anchor id
  layer 3 get_full(ids[]) -> full bodies only for the ids the agent explicitly requests

## Hook-IO discipline (Epic 11)
  capture handlers are PURE: return a structured HookResult; never write stdout/stderr or call exit;
  route all errors through a single emitter so the host agent's stdio protocol is never corrupted
```

---

## EPIC 8 — Discovery-token accounting (prove the memory pays for itself)

**Why (gap):** Mythrax does not record *how much it cost to discover* a fact. Stamping every observation with `discovery_tokens` (the token cost of the tool work that produced it), then at injection time reporting `savings = Σ discovery_tokens − Σ read_tokens`, turns "is the memory layer worth it?" from a vibe into a number, and gives the benchmark in Epic 1 a second axis (token efficiency) beyond recall. (Formula in REFERENCE BEHAVIORS → Token economics.)

### SPEC
- Add an optional `discovery_tokens: Option<u32>` to the episode write path (default `None`).
- At save time, populate it from the originating tool/turn token usage when the caller supplies it (hook adapters from Epic 7 pass it through; absent → `None`).
- At context-injection time (the `pre_invocation_hook` read path), compute `read_tokens` for each injected item and emit a `TokenEconomics { total_read, total_discovery, savings, savings_percent }` block in the hook response payload.

### AST anchors (verify before coding)
- `EpisodeSave` (contracts.rs) — 8 fields today; this adds a 9th. Confirm current field list, then add `discovery_tokens: Option<u32>` to the struct AND to the `save_episode` SurrealDB insert in `db/backend.rs`. **SPEC-GAP** if `EpisodeSave` is not the struct used by `save_episode(&EpisodeSave)`.
- `episode` table in `db/schema.rs` — add `DEFINE FIELD discovery_tokens ON episode TYPE option<int>;`. Confirm `utility option<float>` / `importance DEFAULT 5.0` exist as the pattern to mirror.
- The `pre_invocation_hook` Self-RAG tiering code (read path) — locate where injected items are assembled into the response; add the economics block there.

### TESTS FIRST (`tests/test_discovery_tokens.rs`)
- `test_episode_save_roundtrips_discovery_tokens`: save `EpisodeSave{ discovery_tokens: Some(1234), .. }`, `get_all_episodes()`, assert value preserved; and `None` roundtrips as `None`.
- `test_read_token_estimate_matches_formula`: port `calculateObservationTokens` — `ceil((title.len()+content.len())/CHARS_PER_TOKEN)` with `CHARS_PER_TOKEN = 4`; golden-value test against a fixed fixture.
- `test_token_economics_savings`: 3 injected items with discovery_tokens [1000,500,0] and read estimates summing to 200 → `savings == 1300`, `savings_percent == round(1300/1500*100) == 87`.
- `test_zero_discovery_no_divide_by_zero`: all `None` → `savings_percent == 0`, no panic.

### ACCEPTANCE
- [ ] New field is additive; all existing Epic-1 baseline tests still green (no behavior change to retrieval).
- [ ] Economics block appears in hook response only when ≥1 injected item has `discovery_tokens.is_some()`.
- [ ] `CHARS_PER_TOKEN` is a single named const, not a magic number.

---

## EPIC 9 — Structured observation fields (`facts` / `concepts` / `files_*`) + concept-filtered retrieval

**Why (gap):** Mythrax episodes are `{title, content, ...}` blobs; retrieval is purely embedding similarity. Decomposing each memory into `facts: string[]`, `concepts: string[]`, `files_read[]`, `files_modified[]` enables a cheap structured pre-filter (see REFERENCE BEHAVIORS → Concept-filtered retrieval). For a *coding* harness this is high-value: "what did we learn about `auth.rs`?" is answered by a `files_modified` match, not a fuzzy vector hit. This composes with Epic 4's lexical channel as another non-vector signal.

### SPEC
- Extend the episode record with optional structured arrays: `facts: Vec<String>`, `concepts: Vec<String>`, `files_read: Vec<String>`, `files_modified: Vec<String>` (all default empty).
- Add a structured pre-filter to `search`: when `concepts` or `files` filters are supplied, intersect candidate set before/alongside vector ranking. Vector remains the ranker; structured fields are a **recall booster and filter**, never the sole gate (global invariant).
- Populate these fields opportunistically from hook capture (PostToolUse tool_input/tool_response yields `files_read`/`files_modified` for free; `facts`/`concepts` filled by the existing extraction/dream pass).

### AST anchors
- `EpisodeSave` + `Episode` (contracts.rs): add the four `Vec<String>` fields. Confirmed current `Episode` field list (`id,title,content,source,scope,vault_path,embedding,processed_in_dream,source_episode,last_retrieved_at,utility`) — verified 2.0.0, these are additive.
- `db/schema.rs`: **SPEC-GAP RESOLVED (2.0.0)** — array-field syntax confirmed in-tree (`labels ON entity TYPE array<string>`, embeddings `option<array<float>>`). Add, using the 2.0 idempotent idiom:
  ```
  DEFINE FIELD IF NOT EXISTS facts          ON episode TYPE option<array<string>>;
  DEFINE FIELD IF NOT EXISTS concepts       ON episode TYPE option<array<string>>;
  DEFINE FIELD IF NOT EXISTS files_read     ON episode TYPE option<array<string>>;
  DEFINE FIELD IF NOT EXISTS files_modified ON episode TYPE option<array<string>>;
  DEFINE INDEX IF NOT EXISTS idx_episode_concepts ON episode FIELDS concepts;
  ```
  Mirror `tests/test_schema_upgrades.rs` for the migration test.
- `StorageBackend::search(...)` 11-arg signature (backend.rs @106): **confirmed UNCHANGED in 2.0.0.** Do **NOT** widen the trait. Add a sibling `search_filtered(&self, query:&str, scope:Option<&str>, limit:usize, threshold:f32, concepts:&[String], files:&[String]) -> Result<SearchResponse>` that internally calls the same vector path then intersects on the structured fields. (Alternatively register a `query_symbolic`-style filtered action.) If `search`'s arity differs from the 11-arg anchor at implementation time, emit `SPEC-GAP:` and halt.

### TESTS FIRST (`tests/test_structured_fields.rs`)
- `test_episode_roundtrips_structured_arrays`: save with `files_modified=["auth.rs"]`, read back intact; empty defaults roundtrip.
- `test_concept_prefilter_narrows_candidates`: 3 episodes, only 1 tagged concept "oauth"; filtered search for concept "oauth" returns that 1 even when a vector-closer untagged episode exists → proves filter applied.
- `test_files_modified_filter`: query filtered by `files=["auth.rs"]` returns only episodes that touched it.
- `test_structured_filter_never_empties_floor`: with no concept matches, search falls back to vector results (filter is additive recall, not a hard gate).

### ACCEPTANCE
- [ ] Filter is opt-in; an unfiltered `search` returns byte-identical results to Epic-1 baseline (regression-locked).
- [ ] `recall_any@5` on the held-out split does not regress; concept/file-filtered queries (new eval subset) improve precision.
- [ ] Empty arrays serialize as `[]`, not `null`.

---

## EPIC 10 — 3-layer progressive-disclosure MCP retrieval (`search → timeline → get_full`)

**Why (gap):** Mythrax's MCP exposes search that returns hydrated content directly. A large token saving (~10× in comparable systems) comes from a **three-call protocol** (see REFERENCE BEHAVIORS → 3-layer progressive disclosure): (1) `search` returns a compact index (~50–100 tok/result: id+title+subtitle only), (2) `timeline` returns chronological neighbors of an anchor id, (3) `get_full(ids[])` fetches full bodies only for the handful the agent actually wants. The agent filters before it pays for hydration. This directly reduces the read-token side of Epic 8's ledger.

### SPEC (revised for 2.0.0 MCP architecture)
- **SPEC-GAP RESOLVED:** the MCP surface is NOT 9 flat tools — it is 7 verb-router tools in `mcp_routes.rs`, each dispatching on an inner `action`. Add the three-call protocol as **new `action` arms on `manage_memory`** (routed through `handle_query_memory` @379), NOT as new top-level tools:
  - `action: "search_index"` — args `{query, scope, limit}` -> `[{id, title, subtitle, similarity}]`, **no `content`/`embedding`**, capped per-result tokens.
  - `action: "timeline"` — args `{anchor_id | query, depth_before=3, depth_after=3}` -> `[index rows]` ordered by `created_at` around the anchor.
  - `action: "get_full"` — args `{ids: [String]}` -> `[SearchResult]`, full hydration, batched, the ONLY action that returns `content`.
  Also extend the advertised `tools[]` inputSchema for `manage_memory` (mcp_routes.rs @85) to document the new actions.
- `pre_invocation_hook` automatic injection (handle_pre_invocation_hook @1273) is unchanged; this is the *agent-driven* path.

### AST anchors
- `mcp_routes.rs`: `call_mcp_tool` @238 -> `handle_manage_memory` @366 -> `handle_query_memory` @379. The existing `match action { "search"|"rules"|"nodes"|"root"|"query_symbolic" => ... }` is exactly where the three new arms slot in. `/v1/mcp/call` route already exists in `create_router` (api.rs) — no new REST route needed.
- `SearchResponse` / `SearchResult` (contracts.rs, UNCHANGED): `search_index` returns a **projection** of `SearchResult` (drop `content`,`embedding`,`related_nodes`). Define an `IndexRow` struct rather than mutating `SearchResult`.
- Reuse `StorageBackend::search(...)` (11-arg, confirmed) for `search_index` (project the result); reuse `get_memory_nodes(&[String])` (confirmed signature @147) for `get_full`. For `timeline`, reuse the `followed_by` edge / `created_at` ordering.
- **SPEC-GAP RESOLVED:** `get_memory_nodes(&self, node_ids:&[String])` confirmed present in 2.0.0.

### TESTS FIRST (`tests/test_progressive_disclosure.rs`)
- `test_search_index_omits_content`: assert every `IndexRow` has empty/absent content and embedding; assert per-row serialized size under a token budget constant.
- `test_get_full_hydrates`: ids from `search_index` → `get_full` returns matching `content`.
- `test_timeline_orders_neighbors`: 5 episodes across timestamps; `timeline(anchor=mid, 1, 1)` returns exactly the immediate prior+next by `created_at`.
- `test_index_then_full_token_savings`: assert Σ tokens(search_index) + Σ tokens(get_full for 2 of N) < Σ tokens(search returning all hydrated) — the 10× claim, asserted as a strict inequality on the fixture.

### ACCEPTANCE
- [ ] Existing `/v1/search` and `/v1/mcp/call` behavior unchanged (back-comat).
- [ ] `get_full` is the only tool returning `content`.
- [ ] Token-savings inequality test passes on the standard fixture.

---

## EPIC 11 — Pure hook-IO discipline + SWE-bench A/B eval harness

**Why (gap):** Two distinct contributions.
1. **Hook-IO discipline.** Capture handlers must be PURE: return a structured `HookResult` and **never** write to stdout/stderr or call `exit` — all errors route through one emitter (see REFERENCE BEHAVIORS → Hook-IO discipline). This prevents a memory hook from ever corrupting the host agent's stdio protocol (a real failure mode for Epics 2/6/7). Codify it as a contract + test.
2. **SWE-bench A/B harness.** Epic 1 measures *retrieval* (recall/nDCG). Additionally run **SWE-bench Verified** with vs. without memory and diff the resolve rate. This is the end-to-end "does memory make the coding agent solve more tickets" number — the benchmark you actually care about — and it's the right top-level KPI for the whole program.

### SPEC — Part A: hook-IO contract
- Define a single `HookResult { continue_: bool, suppress_output: bool, exit_code: i32, injected: Option<String> }` returned by **all** Mythrax hook handlers (precompact/stop/precontext).
- Handlers are pure functions of `NormalizedHookInput`; one and only one boundary function (`emit_hook_result`) touches stdout and process exit. On error, handlers return `Err` → boundary maps to a non-blocking result (never crashes the host).

### SPEC — Part B: SWE-bench A/B
- **Honest scoring = use the OFFICIAL SWE-bench harness, do not re-implement "resolved".** SWE-bench's resolution check (apply patch, run the repo's tests, judge pass/fail) is defined and maintained by the dataset authors. Re-implementing it ourselves would be non-comparable and easy to get subtly wrong, so for *this* benchmark we run the authors' published harness as an out-of-process tool against the authors' published dataset. This is the deliberate, scoped exception in the CODE vs DATA ground rule: the official scorer is the benchmark's own reference implementation, kept at arm's length (subprocess, pinned version), and produces the only "% Resolved" we report. We still do NOT pull any *memory-project* code.
- Add `evals/swebench/` with: `run-batch` (produces `predictions.jsonl`), `eval.sh` (wraps the official `python -m swebench.harness.run_evaluation --dataset princeton-nlp/SWE-bench_Verified`), `summarize.py` (tally resolved/unresolved/error → markdown; `--compare OTHER_RUN_ID` diffs resolve-rate delta + per-instance status changes).
- **Pin everything for reproducibility:** record the dataset revision (commit SHA) of `princeton-nlp/SWE-bench_Verified`, the installed `swebench` harness version, and the Mythrax commit in `evals/swebench/README.md` and in each run's manifest. Verify the dataset is the full **500 human-verified** instances before reporting.
- Two run profiles over the **same pinned 500-instance set**: `RUN_ID=baseline` (Mythrax memory disabled) and `RUN_ID=mythrax` (enabled). Report = official **% Resolved** for each + the resolve-rate delta in percentage points, always labeled "SWE-bench Verified, official harness vX, full 500."

### AST anchors
- Mythrax hook handlers from Epics 2/6/7 (to-be-created in this spec) — Part A constrains their return type. **SPEC-GAP** if those epics are skipped (Epic 11A depends on ≥1 hook existing).
- No Mythrax runtime symbol is invented here: Part B is out-of-process tooling that talks to the daemon over HTTP (port 8090) and toggles memory injection. In 2.0 the cleanest A/B switch is the `pre_invocation_hook` MCP tool: `baseline` = host configured WITHOUT the hook (no context injection); `mythrax` = host configured WITH it. (The `config` table / `manage_config` get|set governs LLM routing, not memory on/off, so prefer the hook-presence toggle.) If neither a hook-presence toggle nor a config switch is wireable at implementation time, emit `SPEC-GAP:` and descope Part B to enabled-only as a follow-up.

### TESTS FIRST
- `tests/test_hook_io_discipline.rs`:
  - `test_handler_returns_result_on_error`: feed a handler malformed input → returns `Ok(HookResult{continue_:true, ...})` or `Err` mapped to non-blocking; assert it never panics and process is not exited (call the pure fn directly).
  - `test_emit_is_only_io_boundary`: (review-enforced + unit) handler output contains no stdout writes — assert by capturing and expecting empty.
- `evals/swebench/` smoke test (`smoke-test.sh`): 1-instance dry run asserts `predictions.jsonl` schema (`instance_id`, `model_name_or_path`, `model_patch`) and that `summarize.py` emits a `Resolved: X (Y%)` line.
- `test_summarize_diff_math`: the resolve-rate delta math (own implementation) — fixtures of 2 runs → assert `+N.NN percentage points` and correct per-instance status-change rows.

### ACCEPTANCE
- [ ] Every hook handler returns `HookResult`; grep proves no `println!`/`eprintln!`/`process::exit` inside `hooks/` modules except the single emitter.
- [ ] `summarize.py --compare` produces a resolve-rate delta table on fixture data.
- [ ] Scoring uses the **official `swebench` harness** (not a re-implemented resolver) over the full **500** `princeton-nlp/SWE-bench_Verified` instances; dataset revision, harness version, and Mythrax commit are pinned and recorded.
- [ ] A/B run documented in `evals/swebench/README.md` with the two `RUN_ID`s and the reproducibility manifest; results labeled "SWE-bench Verified, official harness, full 500." This is the headline KPI for the whole advanced-memory program.

---

## EXECUTION ORDER & GLOBAL GATES
1. **Epic 1** → acquire + integrity-check the official 500-question LongMemEval set (pinned), then record the **full500 published BASELINE**. (gate)
2. **Epic 2 + Epic 6** (shared scaffolding) → re-run Epic 1; capture coverage up, recall not down.
3. **Epic 3** → re-run; recall_all holds/improves.
4. **Epic 4** → per-boost ablation; keep only Δ≥0 boosts. Commit ABLATION.md.
5. **Epic 5** → validate on temporal qtypes.
6. **Epic 7** → broaden host support last.
7. **Epic 8** (discovery tokens) → additive; re-run Epic 1, retrieval unchanged, economics block present.
8. **Epic 9** (structured fields) → re-run; unfiltered search byte-identical, filtered subset precision up.
9. **Epic 10** (progressive disclosure) → token-savings inequality test green.
10. **Epic 11A** (hook-IO discipline) → after ≥1 hook epic ships; 11B (SWE-bench A/B) → headline KPI, run last.

**Global invariants (every epic must hold):**
- No symbol/route/field/table invented outside this spec's AST anchors or explicit "to-be-created" signatures. On a missing symbol: emit `SPEC-GAP:` and halt the epic.
- Tests written and failing before implementation; green before acceptance.
- `cargo build` (release, no `bench` feature) excludes Epic 1 bench code.
- `recall_any@5` on the **full official 500** never regresses below the Epic 1 published baseline. (The optional 50/450 internal-gate is CI-only and never a published number.)
- **Publishable-results discipline:** every reported benchmark number names its canonical dataset id + pinned revision + checksum + k/instance count, uses the full official set (LongMemEval 500 / SWE-bench Verified 500), and is labeled honestly (LongMemEval *retrieval* Recall@k/NDCG@k, or SWE-bench % Resolved — never conflated with LongMemEval QA accuracy). No reference-impl convenience split is ever published.
- Verbatim/vector retrieval is always the floor; boosts and closets are signals, never gates.

## SOURCE REFERENCES
The only source repository this spec references is **Mythrax itself**. All third-party design ideas have been distilled into the self-contained REFERENCE BEHAVIORS & PARAMETERS blocks above; no external repositories are to be read, fetched, or depended upon.

Mythrax (verified @ 2.0.0, commit `bc9e282c`): [ARCHITECTURE.md](https://github.com/keith-mcclellan/mythrax/blob/main/ARCHITECTURE.md) · [api.rs](https://github.com/keith-mcclellan/mythrax/blob/main/mythrax-core/src/api.rs) · [mcp_routes.rs](https://github.com/keith-mcclellan/mythrax/blob/main/mythrax-core/src/mcp_routes.rs) · [db/backend.rs](https://github.com/keith-mcclellan/mythrax/blob/main/mythrax-core/src/db/backend.rs) · [db/schema.rs](https://github.com/keith-mcclellan/mythrax/blob/main/mythrax-core/src/db/schema.rs) · [contracts.rs](https://github.com/keith-mcclellan/mythrax/blob/main/mythrax-core/src/contracts.rs) · [vault/watcher.rs](https://github.com/keith-mcclellan/mythrax/blob/main/mythrax-core/src/vault/watcher.rs) · [store.rs](https://github.com/keith-mcclellan/mythrax/blob/main/mythrax-core/src/store.rs) · [cognitive/compactor.rs](https://github.com/keith-mcclellan/mythrax/blob/main/mythrax-core/src/cognitive/compactor.rs) · [tests/test_sigmoid_gated_search.rs](https://github.com/keith-mcclellan/mythrax/blob/main/mythrax-core/tests/test_sigmoid_gated_search.rs) · [tests/test_cycle_proof_traversal.rs](https://github.com/keith-mcclellan/mythrax/blob/main/mythrax-core/tests/test_cycle_proof_traversal.rs) · [tests/test_schema_upgrades.rs](https://github.com/keith-mcclellan/mythrax/blob/main/mythrax-core/tests/test_schema_upgrades.rs)

SWE-bench Verified (Epic 11B) is the public, standard coding benchmark dataset `princeton-nlp/SWE-bench_Verified` — a third-party eval dataset, not a memory project; using it as a benchmark is expected and carries no code-reuse concern.
