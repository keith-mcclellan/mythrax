# Tasks: Phased Implementation & Verification Plan

## Phase 1: Code Fixes (Background Sweep & Timeout Adjustments)

### [x] T1: Implement Background Sweep
* **Purpose**: Implement the idle session transcript mining sweep in `run_dream` in [synthesis.rs](file:///Users/keith/Documents/mythrax/mythrax-core/src/cognitive/synthesis.rs).
* **Related Requirements**: In-scope 2, 3, 4, 5, 6.
* **Related Tests**: T3.
* **Actions**:
  - Query registered paths in `synthesis.rs`:
    `SELECT session_id, value AS path, updated_at FROM short_term_memory WHERE key = '_transcript_path';`
  - Get `_last_swept_at` from STM for each session.
  - Sweep files if modified after `_last_swept_at` and inactive for >10 mins, then update `_last_swept_at`.
* **Validation**: Completed and compiled cleanly.

### [x] T2: Adjust CLI Boot Timeout
* **Purpose**: Increase CLI poll check timeout in [main.rs](file:///Users/keith/Documents/mythrax/mythrax-core/src/main.rs) from 5 to 15 seconds.
* **Related Requirements**: Constraints.
* **Actions**:
  - Update `ensure_daemon_active_for_cli` in `main.rs`.
* **Validation**: Completed and compiled cleanly.

### [x] T3: Write Sweep Integration Tests
* **Purpose**: Create automated integration tests to assert the background sweep and cleanup functionality.
* **Related Requirements**: In-scope 7.
* **Actions**:
  - Create [test_abandoned_session_sweep.rs](file:///Users/keith/Documents/mythrax/mythrax-core/tests/test_abandoned_session_sweep.rs).
  - Assert registration, idle sweep, key maintenance, and missing file warnings.
* **Validation**: Integration test passed successfully (`test test_abandoned_session_sweep_lifecycle ... ok`).

### [x] T3b: Parallelize Test Harness
* **Purpose**: Enable parallel test execution to speed up verification runs.
* **Actions**:
  - Install `cargo-nextest` via Homebrew.
  - Implement LOCK file removal and connection retry workaround in `test_rocksdb_connection_and_persistence` to handle heavy parallel load.
* **Validation**: All 164 tests passed concurrently via nextest in 67s.

---

## Phase 2: Documentation Mapping, Assertion Verification, & Review

We will verify each core data flow step sequentially. For each step, we must build the documentation, critically run the tests/commands to verify the assertions, update the documentation in case of divergence, and record the results in our review document.

### [x] T4: Initialize Phase 2 Artifacts & Review Log
* **Purpose**: Create the review log file to track the status of each data flow assertion.
* **Actions**:
  - Create [verification_review.md](file:///Users/keith/Documents/mythrax/specs/docs_and_sweep/verification_review.md) to record the test execution details, divergences, and actions taken for each system flow.

### [ ] T5: Verify Flow 1 (Startup, Bootstrapping & Self-Healing)
* **Actions**:
  - Write expected flow assertions in [ARCHITECTURE.md](file:///Users/keith/Documents/mythrax/ARCHITECTURE.md) and [DEVELOPMENT.md](file:///Users/keith/Documents/mythrax/DEVELOPMENT.md).
  - Explicitly test Flow 1 (lock contention, WAL replay, permissions).
  - Document any divergences in `verification_review.md` and adjust documentation if needed.

### [ ] T6: Verify Flow 2 (Pre-Compaction Hook & Verbatim Ingestion)
* **Actions**:
  - Write expected flow assertions in `ARCHITECTURE.md` and `DEVELOPMENT.md`.
  - Explicitly test Flow 2 (`test_precompact_ingest.rs`).
  - Document any divergences in `verification_review.md` and adjust documentation if needed.

### [ ] T7: Verify Flow 3 (Request Routing & MCP Dispatcher)
* **Actions**:
  - Write expected flow assertions in `ARCHITECTURE.md` and `DEVELOPMENT.md`.
  - Explicitly test Flow 3 (mock endpoints, handler routing).
  - Document any divergences in `verification_review.md` and adjust documentation if needed.

### [ ] T8: Verify Flow 4 (Memory Co-existence & Retrieval Router)
* **Actions**:
  - Write expected flow assertions in `ARCHITECTURE.md` and `DEVELOPMENT.md`.
  - Explicitly test Flow 4 (`test_sigmoid_gated_search.rs`, search modes).
  - Document any divergences in `verification_review.md` and adjust documentation if needed.

### [ ] T9: Verify Flow 5 (Background Sweeps & Compaction Recovery)
* **Actions**:
  - Write expected flow assertions in `ARCHITECTURE.md` and `DEVELOPMENT.md`.
  - Explicitly test Flow 5 (`test_abandoned_session_sweep.rs`).
  - Document any divergences in `verification_review.md` and adjust documentation if needed.

### [ ] T10: Run Full Verification Suite & Final Audit
* **Actions**:
  - Run all tests to ensure zero regressions across the codebase.
  - Finalize `verification_review.md` with PASS/FAIL status.
