# Implementation Plan: Mythrax 3.1 Refactoring & Architecture Evolution

Track ID: `mythrax_31_refactor_20260802`
Type: Refactor & Architecture Evolution

---

## Phase 1: Test Harness Refactoring & Mutation Hardening

- [x] Task: Refactor Test Fixtures & Isolation Infrastructure (Temp Vault & DB Isolation)
  - [x] Implement isolated temp vault root directory helper (`/tmp/mythrax_vault_<track_id>_<test_id>`) using `tempfile::TempDir`
  - [x] Prohibit unit and integration tests from writing or mutating files inside production `mythrax-vault/`
  - [x] Implement isolated temp DB directory helper (`/tmp/mythrax_31_refactor_<test_id>`)
  - [x] Configure `CARGO_TARGET_DIR=target/mythrax_31_refactor` for parallel Nextest execution
  - [x] Update `domain_vault_storage.rs`, `domain_cognitive.rs`, and `domain_search_retrieval.rs` harness setup
  - [x] Purge historical test-poisoned files (`test_hypothesis_pattern_fact.md`, `test_scope/`) from production `mythrax-vault/`

- [x] Task: Enforce Strict Mutation Assertions Across ALL Test Suites
  - [x] Audit and replace permissive `is_ok()`/`is_some()` assertions in `tests/domain_vault_storage.rs` with exact record count and field mutation checks
  - [x] Audit and replace permissive assertions in `tests/domain_cognitive.rs` with strict hypothesis and cluster state mutations
  - [x] Audit and replace permissive assertions in `tests/domain_search_retrieval.rs` with strict RRF score, BM25 rank, and temporal edge assertions
  - [x] Audit and replace permissive assertions in `tests/domain_legacy_aggregators.rs` (168KB — largest test file, high risk of permissive patterns)
  - [x] Audit and replace permissive assertions in `tests/domain_e2e_harness.rs` (61KB)

- [x] Task: Production `unwrap()` and Silenced Error Safety Audit (CTO Critical E-1, SLOP-8)
  - [x] Audit all `unwrap()` calls in non-test `src/` code (40+ files)
  - [x] Audit all `let _ = <critical_operation>.await;` calls (e.g. `save_wiki_node`, `save_episode`) and replace with proper error propagation or `tracing::error!` logging (CTO High SLOP-8)
  - [x] Fix `bm25.rs:L111` — `self.doc_term_freqs.get(doc_id).unwrap()` panics on missing doc_id — replace with `.ok_or_else()` / match
  - [x] Fix `embeddings.rs` — multiple `.unwrap()` in cache init and model loading paths — replace with `?` propagation / `ok_or_else`
  - [x] Convert `block_on` anti-pattern in `ingestion.rs:L1924` test to `#[tokio::test]`

- [x] Task: Phase Verification & Checkpoint (Refer to workflow.md)

---

## Phase 2: Vault Unification & Modular Episode Storage

  - [ ] Write failing test for `manage(action="organize")` migration of legacy flat files into typed directories
  - [ ] Update `manage(action="organize")` and `manage(action="clean")` in `src/vault/operations.rs` to migrate legacy files into `.episodes/` and typed `wiki/` folders
  - [ ] Update `generate_moc` in `src/vault/organization.rs` to render categorized `MOC.md` with subsystem sections and automatic orphan node capturing

- [ ] Task: Early Prompt Consolidation for Token Savings (CTO D-1/D-2 — moved from Phase 5)
  - [ ] Consolidate verbose distillation system prompt in `distillation.rs:L165` (~310 tokens) and reflect.rs prompt (~160 tokens) into `cognitive/prompts.rs` as `build_distillation_prompt()`
  - [ ] Strip emoji decorators, redundant preambles, and duplicate formal notation from extraction prompt in `prompts.rs:L65-L88` (CTO D-3: saves ~40 tokens/call)
  - [ ] Add global token budget cap on transcript summary in `reflect.rs:L73` (CTO D-4: currently 1000 chars/step with no overall cap — add 8000-token total budget)

- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

---

## Phase 3: Push-Based Cloud Brain Event Architecture

- [x] Task: Audit & Categorize All Daemon Polling Loops (CTO Critical C-1, High C-2)
  - [x] Enumerate all 6 polling loops in `daemon.rs` and explicitly categorize each:
    - `daemon.rs:L168` — 2s startup delay → retain as one-shot timer
    - `daemon.rs:L352` — 600s checkpoint daemon → retain as periodic timer
    - `daemon.rs:L399` — 60s embedding cache flusher → retain as periodic timer
    - `daemon.rs:L418` — 60s reflection harvester → CONVERT to Live Query event-driven
    - `daemon.rs:L444` — 86400s daily scheduler → retain as periodic timer
    - `daemon.rs:L531` — 1s dreaming coordinator debounce → CONVERT to broadcast event
  - [x] Enumerate `distillation.rs:L196` — 50ms polling loop (up to 1200 queries over 60s timeout) → CONVERT to Live Query notification

- [x] Task: Implement SurrealDB Live Query Event Listener
  - [x] Write failing unit test for SurrealDB `LIVE SELECT * FROM cognitive_task WHERE status = 'Pending';` stream listener
  - [x] Implement `LIVE SELECT` listener and `tokio::sync::broadcast` event channel in `src/vault/distillation.rs` and `Daemon` state
  - [x] Replace `distillation.rs:L196` polling loop with `tokio::sync::oneshot` or broadcast receiver
  - [x] Update cognitive callback creation to broadcast reactive task events instantly

- [x] Task: Replace Polling Sleep Loops in Daemon
  - [x] Refactor `daemon.rs` reflection harvester loop (L418) to use reactive `select! { msg = rx.recv() => ... }` event handling
  - [x] Refactor dreaming coordinator debounce (L531) to use broadcast subscription
  - [x] Verify 0ms task wake-up latency and 0 idle polling overhead
  - [x] Add disk space check before vault writes and embedding cache flushes (CTO High E-8)

- [x] Task: Phase Verification & Checkpoint (Refer to workflow.md)

---

## Phase 4: MCP Tool Schema & Universal Hook Lifecycle Enforcement

- [x] Task: Implement Programmatic Pre-Flight Memory Gate Enforcement
  - [x] Write failing test in `tests/domain_hooks_models.rs` asserting memory search gate intercepts write operations when memory has not been queried
  - [x] Implement `has_checked_memory` session state tracking in `ApiState` / `mcp_routes.rs`
  - [x] Enforce memory gate interceptor: block modifying tools (`write_file`, `replace_file_content`, `run_command`, `write`, `organize`) until `read(action="search")` or `manage(action="pre_invocation")` is called
  - [x] Automatically inject mandatory Phase 0 Memory Check directive into all subagent prompt templates

- [x] Task: Implement Universal Hook Lifecycle Enforcement (Stop, Post-Invocation, Precompact, Directive Persistence)
  - [x] Write failing test in `tests/domain_hooks_models.rs` asserting automatic `stop` hook transcript mining on session termination (flushing remaining turns without waiting for 15-message interval)
  - [x] Implement automatic `stop` hook fallback pass in `src/hooks/stop.rs` to guarantee zero lost facts/directions on session close
  - [x] Implement `post_invocation` directive auto-detection in `src/hooks/adapters.rs`: inspect user turns for rule keywords (`always`, `never`, `must`, `rule`, `don't forget`) and automatically queue a high-priority `Direction` extraction task if no explicit `write(action="save")` occurred in the turn
  - [x] Enforce memory gate prompt instruction: "When the user specifies a process rule or directive, agents MUST immediately invoke `write(action='save')` to persist it as a Direction node"
  - [x] Implement `post_invocation` synthetic post-turn observation enforcement in `src/hooks/adapters.rs`
  - [x] Implement `precompact` context pressure gate enforcement when token budget exceeds 80% capacity
  - [x] Register Antigravity plugin manifest lifecycle hooks (`on_session_start`, `post_tool_call`, `on_session_stop`, `on_context_pressure`) in `.mythrax-shared/hooks/` and plugin templates for automatic CLI execution

- [x] Task: Expose Search Parameters & Self-Documenting MCP Tool Schemas (CTO Critical E-2, Medium E-9/E-10)
  - [x] Write failing test for MCP `read` tool schema parameter validation in `tests/domain_search_retrieval.rs`
  - [x] Add `include_archived` (boolean), `temporal_anchor` (string UUID), and `full_content` (boolean) to `get_mcp_tools_schema()` in `src/mcp_routes.rs`
  - [x] Add `description` fields to ALL schema properties across `read`, `write`, `manage`, and `agent` tools (~50 properties)
  - [x] Standardize parameter naming convention (snake_case only) — remove duplicate `path`/`AbsolutePath`/`TargetFile` and `start_line`/`StartLine` aliases; handle case normalization in handler code
  - [x] Refactor `strip_diffs()` flag-based approach in `mcp_routes.rs:L41-L68` to a proper state machine to handle nested code fences correctly (CTO Low B-5)
  - [x] Verify AI agents can natively invoke `read` tool with full search parameters

- [x] Task: Implement Graceful Degradation & Health Reporting (CTO Critical E-3, High E-5)
  - [x] Add `degraded_mode: bool` field to `ApiState` — set when embedder fails to load
  - [x] Include warning banner in search results when running BM25-only mode: "⚠️ Vector search unavailable"
  - [x] Extend `/health` endpoint to return JSON component statuses (embedder, database, disk space, background tasks)
  - [x] Log warning on every search call when in degraded mode

- [x] Task: Harden SecretFilter & Adapter Error Messages (CTO High E-4, E-7)
  - [x] Extend `SecretFilter` with regex-based patterns for AWS keys (`AKIA...`), GitHub tokens (`ghp_...`), PEM keys, JWT tokens, unquoted values, env exports
  - [x] Replace `bail!` in `adapt_codex()` and `adapt_cursor()` with user-friendly error messages and supported harness instructions
  - [x] Update stale version references ("v2.1.0" → current)

- [ ] Task: Add MCP API Rate Limiting (CTO Medium E-11)
  - [ ] Add `tower::limit::RateLimitLayer` to Axum router (100 req/s default)

- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

---

## Phase 5: Deliberate Dead Code & Code Complexity Refactoring

- [ ] Task: Critical Code Duplication Elimination (CTO Critical A-1, A-2)
  - [ ] Delete duplicate `cosine_similarity()` in `cognitive/pipeline.rs:L14` and `vault/distillation.rs:L591`; replace all callsites with `crate::math::cosine_similarity`
  - [ ] Unify duplicate `TranscriptStep` and `ToolCall` structs from `hooks/reflect.rs:L14-L28` and `vault/distillation.rs:L33-L47` into a single canonical definition in `contracts.rs` with `#[serde(default)]`
  - [ ] Unify `adapt_claude_code()` and `adapt_gemini()` identical adapter functions into a single generic adapter (CTO B-4)

- [ ] Task: Code Complexity Reduction & Shared Helper Consolidation
  - [ ] Refactor duplicate concept spreading activation and STM candidate injection helpers into shared utilities in `src/retrieval/`
  - [ ] Extract generic `backfill_missing_embeddings<T>()` helper from 3 duplicate loops in `daemon.rs:L172-L318` (CTO A-6)
  - [ ] Replace N+4 individual config queries in swap monitor (`main.rs:L401-L427`) with a single `SELECT ... FROM config:settings` (CTO A-5)
  - [ ] Remove hardcoded swap thresholds in `check_swap_pressure()` — read from config (CTO F-4)
  - [ ] Centralize hardcoded port "8090" into `const DEFAULT_DAEMON_PORT: u16 = 8090;` and a `fn daemon_url()` helper in a shared config module (CTO High SLOP-1)
  - [ ] Add `MYTHRAX_PROXY_URL` env var override for the hardcoded `8080` LLM fallback URL in `api.rs` (CTO High SLOP-2)
  - [ ] Unify duplicate `MAX_HYDRATION_CHARS = 10000` into a single `const` in `mcp_routes.rs` (CTO Medium SLOP-4, SLOP-12)

- [ ] Task: God Module Decomposition (CTO High B-2, F-1, Low F-7)
  - [ ] Decompose `main.rs` (1771 lines) into `cli/onboarding.rs`, `cli/swap_monitor.rs`, `cli/log_writer.rs` with thin dispatcher
  - [ ] Extract `SizeRollingFileWriter` (currently buried in `main.rs`) into `cli/log_writer.rs` as its own module (CTO Low F-7)
  - [ ] Decompose `ingestion.rs` (2327 lines) into `vault/ingestion/{cursor,claude,antigravity,workspace_sync,forge,mod}.rs`

- [ ] Task: Performance Optimization (CTO High C-3, Medium C-4, C-5)
  - [ ] Cache BM25 `OkapiBM25` instance (or `doc_term_freqs`/`df` maps) and invalidate only on corpus changes
  - [ ] Replace unbounded `STEM_CACHE` thread-local HashMap with LRU cache (10K cap) in `bm25.rs:L176`
  - [ ] Eliminate redundant `token.clone()` in BM25 scoring hot path (`bm25.rs:L29`); consider `Arc<str>` for doc IDs

- [ ] Task: Dead Code & Unused Module Elimination
  - [ ] Extract ~120 lines of hardcoded mock LLM responses from `llm/mod.rs:L590-L675` behind `#[cfg(test)]` or a test feature gate (CTO Critical SLOP-6)
  - [ ] Expand eviction stub cleanup to cover all 4 empty `evict()` stubs: `embeddings.rs:L52`, `embeddings.rs:L528`, `llm/mod.rs:L39`, `llm/mod.rs:L43` (CTO Critical SLOP-5, escalated from A-3)
  - [ ] Replace `MYTHRAX_TEST_MOCK` runtime checks with compile-time gates where possible, and add prominent warning log when active (CTO High SLOP-7)
  - [ ] Replace `_ => {}` catch-alls in `manage_handlers.rs` action dispatch and `daemon.rs` stop handler with explicit error logging or bails (CTO High SLOP-9)
  - [ ] Remove exe-path sniffing for test detection in `search_pipeline.rs:L1169-L1180` (CTO High SLOP-11)
  - [ ] Define `const SECONDS_PER_DAY: f64 = 86_400.0;` and replace inline magic numbers across codebase (CTO Medium SLOP-3)
  - [ ] Replace `unsafe { libc::kill(pid, 0) }` in `main.rs:L304-L306` with `nix` crate safe wrapper `nix::sys::signal::kill(Pid, None)` (CTO Low E-12)
  - [ ] Extract duplicate macOS/Linux `disk::monitor` blocks in `daemon.rs:L901-L954` into a single `#[cfg(unix)]` block (CTO Low E-13)
  - [ ] Remove unused variables, dead helper functions, and legacy migration scripts across `mythrax-core`
  - [ ] Move `fix_embeddings.py` to `scripts/`, delete `mock_audit_report.md` and `mythrax_search_history.log`, relocate `2606.11926v1.pdf` (CTO F-6)
  - [ ] Fix `tuned_params.json` triple path fallback to use absolute `$HOME/.mythrax/` path (CTO C-6)
  - [ ] Run compiler hygiene pass (`cargo check --lib`) to verify 0 unused variable warnings

- [ ] Task: Version Bump, Layout Documentation & Release Prep
  - [ ] Update `Cargo.toml` version from `3.0.0` to `3.1.0-alpha` at track start, bump to `3.1.0` on completion (CTO F-8)
  - [ ] Document vault root layout decision explicitly: vault and source share the same root directory; production users should use separate `~/mythrax-vault/` (CTO Medium F-3)
  - [ ] Add vault layout documentation to `conductor/product-guidelines.md` or `README.md` for new user onboarding

- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

---

## Phase 6: Independent Adversarial Review & Anti-Slop Validation Gate

- [ ] Task: Independent Code & Test Review (Recursive Criticism)
  - [ ] Invoke independent Adversarial Reviewer agent (`conductor-review`) to perform recursive criticism on all Phase 1-5 changes
  - [ ] Verify 0 `// TODO` stubs, 0 placeholder functions, 0 dead code, and 0 silenced `let _ =` errors across the codebase
  - [ ] Run full 310-test suite via `MYTHRAX_TEST_MOCK=1 cargo nextest run` to verify 100% clean test execution
  - [ ] Run Dev50 benchmark suite (`scripts/verify_dev50.sh`) to verify zero recall/precision regression
  - [ ] Run release compilation (`cargo build --release`) and verify 0 warnings and 0 mock LLM responses in production binary

- [ ] Task: Track Final Verification & Checkpoint (Refer to workflow.md)
