# Test Plan: Documentation Update & Background Transcript Sweep

## Unit Tests
* Validate `should_save` threshold intervals in [stop.rs](file:///Users/keith/Documents/mythrax/mythrax-core/src/hooks/stop.rs) are untouched.
* Validate transcript adapters correctly parse payload schemas for Gemini, Claude Code, and other hosts.

## Integration Tests
* **Abandoned Session Sweep Test**:
  Add an integration test in `tests/test_compactor.rs` (or a dedicated test file `tests/test_abandoned_session_sweep.rs`) that:
  1. Spawns an in-memory SurrealDB backend and a temporary Markdown store.
  2. Stashes a valid transcript path under STM key `_transcript_path` for a mock session.
  3. Writes two trailing turns (not yet saved to DB) to the transcript file.
  4. Artificially sets the STM records' `updated_at` time to $>10$ minutes ago to simulate an idle/abandoned state.
  5. Triggers `DreamCoordinator::run_dream` on the compactor.
  6. Asserts that the compactor successfully background-mines the transcript file (episodes count increases).
  7. Asserts that the `_transcript_path` key is deleted from the session's STM table.
  8. Asserts that the newly mined episodes are processed and compacted.

## Acceptance Tests
* Run `cargo test` in `mythrax-core` and verify all tests pass.
* Verify the overwritten `ARCHITECTURE.md` and `DEVELOPMENT.md` files are present and match system data flows exactly.

## Edge Cases
* **Missing Transcript File**:
  Test that if a session's `_transcript_path` points to a deleted file, `run_dream` does not crash, logs a warning, and deletes the STM registry key to prevent stuck sessions.
* **Active Session Lock**:
  Test that if a session was updated less than 10 minutes ago, `run_dream` skips mining it, keeping the key intact for the next cycle.
