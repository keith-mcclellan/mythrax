# Implementation Plan: Mythrax 3.1 Refactoring & Architecture Evolution

Track ID: `mythrax_31_refactor_20260802`
Type: Refactor & Architecture Evolution

---

## Phase 1: Test Harness Refactoring & Mutation Hardening

- [ ] Task: Refactor Test Fixtures & Isolation Infrastructure (Temp Vault & DB Isolation)
  - [ ] Implement isolated temp vault root directory helper (`/tmp/mythrax_vault_<track_id>_<test_id>`) using `tempfile::TempDir`
  - [ ] Prohibit unit and integration tests from writing or mutating files inside production `mythrax-vault/`
  - [ ] Implement isolated temp DB directory helper (`/tmp/mythrax_31_refactor_<test_id>`)
  - [ ] Configure `CARGO_TARGET_DIR=target/mythrax_31_refactor` for parallel Nextest execution
  - [ ] Update `domain_vault_storage.rs`, `domain_cognitive.rs`, and `domain_search_retrieval.rs` harness setup
  - [ ] Purge historical test-poisoned files (`test_hypothesis_pattern_fact.md`, `test_scope/`) from production `mythrax-vault/`

- [ ] Task: Enforce Strict Mutation Assertions Across Core Test Suites
  - [ ] Audit and replace permissive `is_ok()`/`is_some()` assertions in `tests/domain_vault_storage.rs` with exact record count and field mutation checks
  - [ ] Audit and replace permissive assertions in `tests/domain_cognitive.rs` with strict hypothesis and cluster state mutations
  - [ ] Audit and replace permissive assertions in `tests/domain_search_retrieval.rs` with strict RRF score, BM25 rank, and temporal edge assertions

- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

---

## Phase 2: Vault Unification & Modular Episode Storage

- [ ] Task: Implement Typed Directory Routing & 65-Char + CRC32 Slug Capping
  - [ ] Write failing unit test for `organize_file` slug capping and typed path routing in `src/vault/organization.rs`
  - [ ] Implement `<slug_65>-<crc32>.md` slug capping with word-boundary trimming in `src/vault/organization.rs`
  - [ ] Implement typed directory routing (`wiki/<scope>/{references/{ast,docs,forged},facts,insights,directions,hypotheses}/`) and `wisdom/skills/`

- [ ] Task: Implement Modular Hidden Episode Storage
  - [ ] Write failing test for modular hidden episode storage under `mythrax-vault/.episodes/<YYYY-MM>/`
  - [ ] Update `save_episode_bidirectional` in `src/vault/watcher.rs` and `src/vault/ingestion.rs` to write modular hidden episode markdown files
  - [ ] Update `TargetResolveCache` preloading to inspect typed subdirectories alongside legacy paths

- [ ] Task: Implement Vault Organization Migration & MOC Generation
  - [ ] Write failing test for `manage(action="organize")` migration of legacy flat files into typed directories
  - [ ] Update `manage(action="organize")` and `manage(action="clean")` in `src/vault/operations.rs` to migrate legacy files into `.episodes/` and typed `wiki/` folders
  - [ ] Update `generate_moc` in `src/vault/organization.rs` to render categorized `MOC.md` with subsystem sections and automatic orphan node capturing

- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

---

## Phase 3: Push-Based Cloud Brain Event Architecture

- [ ] Task: Implement SurrealDB Live Query Event Listener
  - [ ] Write failing unit test for SurrealDB `LIVE SELECT * FROM cognitive_task WHERE status = 'Pending';` stream listener
  - [ ] Implement `LIVE SELECT` listener and `tokio::sync::broadcast` event channel in `src/vault/distillation.rs` and `Daemon` state
  - [ ] Update cognitive callback creation to broadcast reactive task events instantly

- [ ] Task: Replace Polling Sleep Loops in Daemon
  - [ ] Refactor `daemon.rs` reflection harvester loop to use reactive `select! { msg = rx.recv() => ... }` event handling
  - [ ] Verify 0ms task wake-up latency and 0 idle polling overhead

- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

---

## Phase 4: MCP Tool Schema & Universal Hook Lifecycle Enforcement

- [ ] Task: Implement Programmatic Pre-Flight Memory Gate Enforcement
  - [ ] Write failing test in `tests/domain_hooks_models.rs` asserting memory search gate intercepts write operations when memory has not been queried
  - [ ] Implement `has_checked_memory` session state tracking in `ApiState` / `mcp_routes.rs`
  - [ ] Enforce memory gate interceptor: block modifying tools (`write_file`, `replace_file_content`, `run_command`, `write`, `organize`) until `read(action="search")` or `manage(action="pre_invocation")` is called
  - [ ] Automatically inject mandatory Phase 0 Memory Check directive into all subagent prompt templates

- [ ] Task: Implement Universal Hook Lifecycle Enforcement (Stop, Post-Invocation, Precompact)
  - [ ] Write failing test in `tests/domain_hooks_models.rs` asserting automatic `stop` hook transcript mining on session termination
  - [ ] Implement automatic `stop` hook fallback pass in `src/hooks/stop.rs` to guarantee zero lost facts on session close
  - [ ] Implement `post_invocation` synthetic post-turn observation enforcement in `src/hooks/adapters.rs`
  - [ ] Implement `precompact` context pressure gate enforcement when token budget exceeds 80% capacity
  - [ ] Register Antigravity plugin manifest lifecycle hooks (`on_session_start`, `post_tool_call`, `on_session_stop`, `on_context_pressure`) in `.mythrax-shared/hooks/` and plugin templates for automatic CLI execution

- [ ] Task: Expose Search Parameters in MCP Tool Schemas
  - [ ] Write failing test for MCP `read` tool schema parameter validation in `tests/domain_search_retrieval.rs`
  - [ ] Update `get_mcp_tools_schema()` in `src/mcp_routes.rs` to add `include_archived`, `temporal_anchor`, and `full_content` properties
  - [ ] Verify AI agents can natively invoke `read` tool with full search parameters

- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

---

## Phase 5: Deliberate Dead Code & Code Complexity Refactoring

- [ ] Task: Code Complexity Reduction & Shared Helper Consolidation
  - [ ] Consolidate duplicated distillation system prompts across `src/hooks/reflect.rs` and `src/vault/distillation.rs` into `src/cognitive/prompts.rs`
  - [ ] Refactor duplicate concept spreading activation and STM candidate injection helpers into shared utilities in `src/retrieval/`

- [ ] Task: Dead Code & Unused Module Elimination
  - [ ] Remove unused variables, dead helper functions, and legacy migration scripts across `mythrax-core`
  - [ ] Run compiler hygiene pass (`cargo check --lib`) to verify 0 unused variable warnings

- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)
