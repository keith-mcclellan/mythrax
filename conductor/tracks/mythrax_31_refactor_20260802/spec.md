# Specification: Mythrax 3.1 Refactoring & Architecture Evolution

## Track Overview
- **Track ID:** `mythrax_31_refactor_20260802`
- **Track Type:** Refactor & Architecture Evolution
- **Status:** New

## 1. Objectives & Summary
The Mythrax 3.1 Track stabilizes, refactors, and evolves the Mythrax Sidecar Intelligence Engine into a clean, mutation-tested, push-driven architecture with a unified vault directory layout:
1. **Phase 1 (Test Harness Refactoring & Mutation Hardening):** Eliminate overly permissive tests by replacing generic truthy/non-null assertions with strict exact value, field mutation, and record-count assertions. Refactor test fixtures to execute in isolated temp DB directories (`/tmp/<track_id>`) and isolated `CARGO_TARGET_DIR` targets, enforcing parallel execution with `cargo nextest run`.
2. **Phase 2 (Vault Unification & Modular Episode Storage):** Reorganize flat vault file sprawl into a typed domain hierarchy under `wiki/<scope>/` (`references/{ast,docs,forged}`, `facts/`, `insights/`, `directions/`, `hypotheses/`). Move raw episode disk storage to modular hidden monthly directories (`mythrax-vault/.episodes/<YYYY-MM>/`) to keep Obsidian's Graph View 100% clean. Enforce 65-character + 4-char CRC32 hash slug capping (`<slug_65>-<crc32>.md`) to eliminate filename collisions.
3. **Phase 3 (Push-Based Cloud Brain Event Architecture):** Replace 15s/300s polling sleep loops in `daemon.rs` with SurrealDB Live Queries (`LIVE SELECT * FROM cognitive_task WHERE status = 'Pending';`) over WebSockets and an in-memory `tokio::sync::broadcast` event bus, waking Cloud Brain handlers with 0ms latency and 0 idle polling token burn.
4. **Phase 4 (MCP Tool Schema & Search Capability Enhancements):** Expose native search capabilities (`include_archived`, `temporal_anchor`, `full_content`) in `src/mcp_routes.rs` (`get_mcp_tools_schema()`) so AI subagents can natively invoke full 6-signal hybrid search features.
5. **Phase 5 (Deliberate Dead Code & Code Complexity Refactoring):** Audit and purge dead code, unused variables, legacy scripts, and redundant distillation prompt strings across `mythrax-core`. Consolidate duplicated search helpers (concept spreading activation, STM candidate injection) into shared utilities.

---

## 2. Functional Requirements

### 2.1 Test Harness & Mutation Hardening
- **Isolated Temporary Vault Roots:** Unit and integration tests must initialize temporary vault root directories (`tempfile::TempDir`) for all file operations. Tests are strictly forbidden from writing or mutating files inside production `mythrax-vault/` or `$HOME/mythrax-vault/`. Historical test files in the production vault must be purged.
- **Exact Mutation Assertions:** Unit and integration tests must assert exact record counts, exact field values, and graph edge mutations. Prohibit `assert!(result.is_ok())` or `assert!(item.is_some())` without inspecting payload content.
- **Test Isolation:** All parallel test execution runs must use `CARGO_TARGET_DIR=target/mythrax_31_refactor` and isolated temp DB directories (`/tmp/mythrax_31_refactor`).
- **Parallel Nextest Enforcement:** All test runs must execute via `MYTHRAX_TEST_MOCK=1 cargo nextest run`.

### 2.2 Architectural Guardrails & Anti-Patterns to Avoid (From Mythrax Memory)
- **Anti-Pattern 1 (Vault & DB Poisoning):** Writing test notes or DB records into `$HOME/mythrax-vault/`. Remedy: Force `tempfile::TempDir` RAII scope guards on all test stores.
- **Anti-Pattern 2 (Permissive Assertions):** Using truthy `is_ok()`/`is_some()` assertions. Remedy: Assert exact mutation values and edge links.
- **Anti-Pattern 3 (Sync/Async Bridge Wrappers):** Creating `_async` methods or using `futures::executor::block_on` fallbacks. Remedy: Refactor trait signatures natively with `async fn`.
- **Anti-Pattern 4 (Lock Contention across I/O):** Holding Mutex locks while doing disk I/O or DB queries. Remedy: Extract local variables, drop lock, then perform I/O.
- **Anti-Pattern 5 ($O(N)$ Hot-Path Scans):** Linear iteration in hot loops. Remedy: Use $O(1)$ LRU caches and bulk evictions.

### 2.2 Vault Storage & Directory Organization
- **Typed Directory Hierarchy:**
  - `wiki/<scope>/references/ast/` — AST code symbol pages
  - `wiki/<scope>/references/docs/` — Workspace documentation & specs
  - `wiki/<scope>/references/forged/` — Forged paper & external reference assets
  - `wisdom/skills/` — Reusable agent skills
  - `wiki/<scope>/facts/` — Extracted atomic `Fact` nodes
  - `wiki/<scope>/insights/` — Synthesized `Insight` nodes
  - `wiki/<scope>/directions/` — User directives & preference nodes (`Direction`)
  - `wiki/<scope>/hypotheses/` — Arbor hypothesis tree nodes
- **Slug Capping & Collision Protection:** Slugs must be capped at 65 characters + 4-char CRC32 hash (`<slug_65>-<crc32>.md`).
- **Modular Hidden Episode Storage:** Raw episode files must be stored as modular individual markdown files under `mythrax-vault/.episodes/<YYYY-MM>/<slug>_<hash>.md` (hidden dot-folder, excluded from Obsidian Graph View).
- **TargetResolveCache & Backwards Compatibility:** `TargetResolveCache` preloading must resolve legacy flat paths (`wiki/<scope>/...`, `reference/<scope>/...`) alongside new typed paths (`wiki/<scope>/{type}/...`) seamlessly.

### 2.3 Push-Based Event Architecture
- **SurrealDB Live Query Listener:** Implement `LIVE SELECT * FROM cognitive_task WHERE status = 'Pending';` over WebSockets in `src/vault/distillation.rs`.
- **Tokio Broadcast Event Bus:** Implement a central `tokio::sync::broadcast` channel for reactive task dispatching.
- **Daemon Loop Cleanup:** Eliminate 60-second polling sleep loops in `daemon.rs`.

### 2.4 MCP Tool Schema & Universal Hook Lifecycle Enforcement
- **Pre-Invocation Memory Gate:** Track session memory status (`has_checked_memory`). Intercept write/modifying tool calls until `read(action="search")` or `manage(action="pre_invocation")` is executed.
- **Stop Hook Transcript Mining Enforcement:** Implement automatic `stop` hook fallback in `src/hooks/stop.rs` to guarantee raw transcript turns are mined for facts before session termination.
- **Post-Invocation Observation Enforcement:** Automatically append synthetic post-turn observation logging in `src/hooks/adapters.rs` after tool executions.
- **Precompact Context Pressure Gate:** Trigger automatic context compaction and virtual skeleton paging when token window usage exceeds 80% capacity.
- **Antigravity Plugin Lifecycle Hook Bindings:** Register plugin lifecycle hooks (`on_session_start` → `pre_invocation`, `post_tool_call` → `post_invocation`, `on_session_stop` → `stop`, `on_context_pressure` → `precompact`) in `.mythrax-shared/hooks/` so the IDE/CLI executes the daemon binary automatically without relying on LLM memory.
- **MCP Schema Parameters:** Update `get_mcp_tools_schema()` in `src/mcp_routes.rs` to include `include_archived`, `temporal_anchor`, and `full_content`.

### 2.5 Deliberate Dead Code & Complexity Reduction
- Remove unused variables, dead functions, and legacy scripts across `mythrax-core`.
- Consolidate duplicated prompt strings across `src/hooks/reflect.rs` and `src/vault/distillation.rs` into `src/cognitive/prompts.rs`.

---

## 3. Non-Functional Requirements & Performance
- **Dev50 Regression Gate:** Dev50 recall metrics (Recall_Any@5, Recall_All@5, nDCG@10) must not regress below baseline, and average latency must remain within 115% of baseline.
- **Graph Traversability:** 100% Obsidian graph traversability must be maintained via valid `[[wikilinks]]`.
- **Zero Memory Leaks & VRAM Thrashing:** Local MLX transformer models must respect single-permit GPU semaphores.

---

## 4. Acceptance Criteria
- [ ] `MYTHRAX_TEST_MOCK=1 cargo nextest run` passes 100% with strict mutation assertions.
- [ ] Dev50 benchmark script `scripts/verify_dev50.sh` returns PASS.
- [ ] `manage(action="organize")` migrates vault files into typed subdirectories under `wiki/<scope>/` and modular `.episodes/<YYYY-MM>/`.
- [ ] `manage(action="audit_compliance")` returns `daemon_ok: true`.
- [ ] AI subagents can natively invoke `read` tool with `include_archived`, `temporal_anchor`, and `full_content`.
