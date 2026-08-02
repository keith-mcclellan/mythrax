# Implementation Plan

## Phase 1: Anti-Slop Foundation & Performance
- [~] Task: Fix $O(N)$ scan in `src/vault/watcher.rs`
  - [~] Write failing tests for watcher event timeout logic
  - [ ] Implement `BTreeMap<Instant, PathBuf>`
  - [ ] Refactor and verify coverage
- [ ] Task: Eradicate panics in `src/api.rs`, `src/auth.rs`, `src/bench/`
  - [ ] Map custom Error types
  - [ ] Replace `.unwrap()`/`.expect()` with safe `Result` propagation
  - [ ] Verify test suite coverage for error branches
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 2: Database Monolith Decomposition
- [ ] Task: Create `backend` module hierarchy
  - [ ] Create `src/db/backend/mod.rs`, `connection.rs`, and `migrations.rs`
  - [ ] Migrate SurrealDB lifecycle and connection pooling
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 3: Domain Queries Extraction
- [ ] Task: Extract Domain Queries
  - [ ] Create `src/db/queries/` and extract domain-specific logic
  - [ ] Decompose `src/db/crud_operations.rs` and `src/db/search_pipeline.rs`
  - [ ] Update all downstream imports natively
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 4: Cognitive Pipeline Refactor & Safety Guards
- [ ] Task: Decompose `pipeline.rs`
  - [ ] Create `src/cognitive/pipeline/orchestrator.rs` and `signals/` directory
- [ ] Task: Enforce RAII and Eviction Safety
  - [ ] Implement `Drop` scope guards for `IS_INGESTING` and temp states
  - [ ] Add explicit `evict()` and sync disk flushing for VRAM/LRU caches
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 5: Core Application & Service Layer Extraction
- [ ] Task: Decompose `main.rs` and `api.rs`
  - [ ] Establish `src/daemon/` core
- [ ] Task: Define Service Layer DTOs
  - [ ] Create `src/mcp_routes/dtos.rs`
- [ ] Task: Extract Vault & Cognitive Services
  - [ ] Refactor MCP Handlers to act as thin wrappers
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)

## Phase 6: Documentation Standards
- [ ] Task: Rewrite Concision and Algorithm Docs
  - [ ] Sweep docstrings across new modules to enforce `docs:write-concisely`
  - [ ] Add architectural overviews for DBScan and Spreading Activation
- [ ] Task: Phase Verification & Checkpoint (Refer to workflow.md)
