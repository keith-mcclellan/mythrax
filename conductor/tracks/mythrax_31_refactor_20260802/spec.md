# Specification: Mythrax 3.1 Refactoring & Architecture Evolution

## Track Overview
- **Track ID:** `mythrax_31_refactor_20260802`
- **Track Type:** Refactor & Architecture Evolution
- **Status:** Approved (CTO Audited & Multi-Pass Verified)

## 1. Objectives & Summary
The Mythrax 3.1 Track stabilizes, refactors, and evolves the Mythrax Sidecar Intelligence Engine into a clean, mutation-tested, push-driven architecture with a unified vault directory layout, production readiness, and zero anti-slop:
1. **Phase 1 (Test Harness Refactoring & Mutation Hardening & Error Safety Audit):** Eliminate overly permissive tests by replacing generic truthy/non-null assertions with strict exact value, field mutation, and record-count assertions. Refactor test fixtures to execute in isolated temp DB directories (`/tmp/<track_id>`) and isolated `CARGO_TARGET_DIR` targets, enforcing parallel execution with `cargo nextest run`. Audit all production `unwrap()` calls and silenced error handlers (`let _ = ...`).
2. **Phase 2 (Vault Unification, Migration Safety & Token Savings):** Reorganize flat vault file sprawl into a typed domain hierarchy under `wiki/<scope>/` (`references/{ast,docs,forged}`, `facts/`, `insights/`, `directions/`, `hypotheses/`). Move raw episode disk storage to modular hidden monthly directories (`mythrax-vault/.episodes/<YYYY-MM>/`) to keep Obsidian's Graph View 100% clean. Enforce 65-character + 4-char CRC32 hash slug capping (`<slug_65>-<crc32>.md`). Include vault backup verification, version markers, post-migration verification, and rollback capabilities. Early prompt consolidation for token optimization.
3. **Phase 3 (Push-Based Cloud Brain Event Architecture):** Replace 15s/300s polling sleep loops in `daemon.rs` and 50ms polling loops in `distillation.rs` with SurrealDB Live Queries (`LIVE SELECT * FROM cognitive_task WHERE status = 'Pending';`) over WebSockets and an in-memory `tokio::sync::broadcast` event bus, waking Cloud Brain handlers with 0ms latency and 0 idle polling token burn. Include pre-write disk space checks.
4. **Phase 4 (MCP Tool Schema, Health Observability & Universal Hook Lifecycle Enforcement):** Expose native search capabilities (`include_archived`, `temporal_anchor`, `full_content`), property descriptions, and parameter normalization in `src/mcp_routes.rs` (`get_mcp_tools_schema()`). Implement graceful degradation notifications when embedder is unavailable, extended JSON `/health` endpoint, SecretFilter pattern hardening, user-friendly adapter errors, and MCP rate limiting (100 req/s).
5. **Phase 5 (Deliberate Dead Code, Anti-Slop & God Module Decomposition):** Audit and purge dead code, 4 empty `evict()` stubs, ~120 lines of production-leaked hardcoded mock responses in `llm/mod.rs`, `MYTHRAX_TEST_MOCK` runtime leaks, silent match catch-alls (`_ => {}`), exe-path test sniffing, hardcoded ports ("8090", "8080"), and magic numbers (`86400`). Decompose `main.rs` (1771 lines) and `ingestion.rs` (2327 lines) into modular sub-packages.

---

## 2. Functional Requirements

### 2.1 Test Harness, Mutation Hardening & Error Safety
- **Isolated Temporary Vault Roots:** Unit and integration tests must initialize temporary vault root directories (`tempfile::TempDir`) for all file operations. Tests are strictly forbidden from writing or mutating files inside production `mythrax-vault/` or `$HOME/mythrax-vault/`. Historical test files in the production vault must be purged.
- **Exact Mutation Assertions:** Unit and integration tests must assert exact record counts, exact field values, and graph edge mutations across all domain test suites (`domain_vault_storage.rs`, `domain_cognitive.rs`, `domain_search_retrieval.rs`, `domain_legacy_aggregators.rs`, `domain_e2e_harness.rs`). Prohibit `assert!(result.is_ok())` or `assert!(item.is_some())` without inspecting payload content.
- **Production `unwrap()` & Silenced Error Safety Audit:** Audit and replace all `unwrap()` calls in production `src/` code (e.g. `bm25.rs:L111` panics on missing `doc_id`, `embeddings.rs` cache init) with Result propagation (`?` or `.ok_or_else()`). Audit 40+ `let _ = <async_db_op>.await;` calls (e.g. `save_wiki_node`, `save_episode`) and replace with proper error propagation or `tracing::error!` logging. Convert `block_on` test anti-patterns (`ingestion.rs:L1924`) to `#[tokio::test]`.
- **Test Isolation & Nextest:** All test runs must use `CARGO_TARGET_DIR=target/mythrax_31_refactor`, isolated temp DB directories (`/tmp/mythrax_31_refactor`), and execute via `MYTHRAX_TEST_MOCK=1 cargo nextest run`.

### 2.2 Vault Storage, Migration Safety & Token Savings
- **Typed Directory Hierarchy:**
  - `wiki/<scope>/references/ast/` — AST code symbol pages
  - `wiki/<scope>/references/docs/` — Workspace documentation & specs
  - `wiki/<scope>/references/forged/` — Forged paper & external reference assets
  - `wisdom/skills/` — Reusable agent skills
  - `wiki/<scope>/facts/` — Extracted atomic `Fact` nodes
  - `wiki/<scope>/insights/` — Synthesized `Insight` nodes
  - `wiki/<scope>/directions/` — User directives & preference nodes (`Direction`)
  - `wiki/<scope>/hypotheses/` — Arbor hypothesis tree nodes
- **Canonical Slug Capping & Consolidations:** Slugs must be capped at 65 characters + 4-char CRC32 hash (`<slug_65>-<crc32>.md`) via `src/vault/organization.rs`. Unify `slugify_title()` and `derive_slug()` into a single canonical `slug::slugify()` helper.
- **Modular Hidden Episode Storage:** Raw episode files must be stored as modular individual markdown files under `mythrax-vault/.episodes/<YYYY-MM>/<slug>_<hash>.md` (hidden dot-folder, excluded from Obsidian Graph View).
- **Migration Safety & Rollback:** Implement vault pre-migration backup verification (`backup_vault_folders()`), version marker (`.mythrax/vault_version = "3.1"`), post-migration path verification against SurrealDB records, and `manage(action="rollback_organize")`.
- **Early Token Savings:** Consolidate verbose distillation prompts (`distillation.rs`, `reflect.rs`) into `cognitive/prompts.rs`, strip redundant preambles, and enforce global token budget caps on transcript summaries.

### 2.3 Push-Based Event Architecture
- **SurrealDB Live Query Listener:** Implement `LIVE SELECT * FROM cognitive_task WHERE status = 'Pending';` over WebSockets in `src/vault/distillation.rs` and `Daemon` state.
- **Tokio Broadcast Event Bus:** Implement a central `tokio::sync::broadcast` channel for reactive task dispatching.
- **Daemon Loop Categorization:** Audit all background loops — convert reflection harvester (L418), dreaming coordinator (L531), and distillation task wait (L196) to event-driven listeners; retain periodic timers with justification.
- **Pre-Write Disk Checks:** Enforce `check_disk_space()` before vault writes and embedding cache flushes.

### 2.4 MCP Tool Schema, Health Observability & Universal Hook Lifecycle Enforcement
- **Pre-Invocation Memory Gate:** Track session memory status (`has_checked_memory`). Intercept write/modifying tool calls until `read(action="search")` or `manage(action="pre_invocation")` is executed.
- **Stop Hook Transcript Mining Enforcement:** Implement automatic `stop` hook fallback in `src/hooks/stop.rs` to guarantee raw transcript turns are mined for facts before session termination.
- **Post-Invocation Observation Enforcement:** Automatically append synthetic post-turn observation logging in `src/hooks/adapters.rs` after tool executions.
- **Precompact Context Pressure Gate:** Trigger automatic context compaction and virtual skeleton paging when token window usage exceeds 80% capacity.
- **Antigravity Plugin Lifecycle Hook Bindings:** Register plugin lifecycle hooks (`on_session_start` → `pre_invocation`, `post_tool_call` → `post_invocation`, `on_session_stop` → `stop`, `on_context_pressure` → `precompact`) in `.mythrax-shared/hooks/`.
- **Self-Documenting MCP Tool Schemas:** Update `get_mcp_tools_schema()` in `src/mcp_routes.rs` to expose `include_archived`, `temporal_anchor`, and `full_content`, add `description` text to all ~50 tool properties, standardize on snake_case parameter names, and convert `strip_diffs()` to a robust state machine.
- **Graceful Degradation & Health Endpoint:** Set `degraded_mode: bool` when embedder is unavailable, return warning banners in search results, log warnings on search calls, and extend `/health` endpoint to return JSON status for embedder, DB, disk space, and background tasks.
- **SecretFilter & Rate Limiting:** Extend `SecretFilter` with regex patterns for AWS keys, GitHub tokens, PEM keys, JWT tokens, unquoted secrets, and env exports. Add `tower::limit::RateLimitLayer` (100 req/s) to Axum router. Replace `bail!` in unsupported harness adapters with friendly user guidance.

### 2.5 Deliberate Dead Code, Anti-Slop & God Module Decomposition
- **Code Duplication Elimination:** Delete duplicate `cosine_similarity()` definitions in `pipeline.rs` and `distillation.rs` (use `crate::math::cosine_similarity`). Unify duplicate `TranscriptStep` and `ToolCall` structs into `contracts.rs`. Unify `adapt_claude_code()` and `adapt_gemini()`.
- **Anti-Slop Cleanups:**
  - Extract ~120 lines of hardcoded mock LLM responses in `llm/mod.rs:L590-L675` behind `#[cfg(test)]` or a feature gate.
  - Fix all 4 empty `evict()` stubs across `embeddings.rs` and `llm/mod.rs`.
  - Replace `MYTHRAX_TEST_MOCK` runtime checks with compile-time gates where possible.
  - Replace `_ => {}` catch-alls in dispatch with explicit logging/error propagation.
  - Remove exe-path test sniffing in `search_pipeline.rs`.
  - Centralize default port `"8090"` and LLM fallback URL `"8080"` with env var overrides.
  - Centralize `SECONDS_PER_DAY` (86400) and `MAX_HYDRATION_CHARS` (10000).
  - Replace `unsafe { libc::kill }` with `nix` crate safe wrapper.
  - Clean up legacy root files (`fix_embeddings.py`, `mock_audit_report.md`, `2606.11926v1.pdf`).
- **God Module Decomposition:**
  - Decompose `main.rs` (1771 lines) into `cli/onboarding.rs`, `cli/swap_monitor.rs`, `cli/log_writer.rs` (`SizeRollingFileWriter`), and a thin dispatcher.
  - Decompose `ingestion.rs` (2327 lines) into `vault/ingestion/{cursor,claude,antigravity,workspace_sync,forge,mod}.rs`.

---

## 3. Non-Functional Requirements & Performance
- **Dev50 Regression Gate:** Dev50 recall metrics (Recall_Any@5, Recall_All@5, nDCG@10) must not regress below baseline, and average latency must remain within 115% of baseline.
- **BM25 Caching & Memory Efficiency:** Cache BM25 index structures (`OkapiBM25`), bound thread-local stem cache (`STEM_CACHE`) to 10K entries, and eliminate redundant string cloning in scoring hot-paths.
- **Graph Traversability:** 100% Obsidian graph traversability must be maintained via valid `[[wikilinks]]`.
- **Zero Memory Leaks & VRAM Thrashing:** Local MLX transformer models must respect single-permit GPU semaphores.

---

## 4. Acceptance Criteria
- [ ] `MYTHRAX_TEST_MOCK=1 cargo nextest run` passes 100% with strict mutation assertions and zero test vault/DB poisoning.
- [ ] Production code audit verifies 0 unsafe `unwrap()` panics on database/retrieval paths and 0 silenced critical DB errors.
- [ ] Dev50 benchmark script `scripts/verify_dev50.sh` returns PASS.
- [ ] `manage(action="organize")` migrates vault files into typed subdirectories under `wiki/<scope>/` and modular `.episodes/<YYYY-MM>/` with backup and rollback support.
- [ ] `manage(action="audit_compliance")` returns `daemon_ok: true`.
- [ ] AI subagents natively invoke `read` tool with `include_archived`, `temporal_anchor`, and `full_content` with self-documenting property descriptions.
- [ ] Production binary built with `cargo build --release` contains 0 hardcoded mock LLM responses or empty eviction stubs.
