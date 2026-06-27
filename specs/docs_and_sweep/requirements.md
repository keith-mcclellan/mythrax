# Requirements: Documentation Update & Background Transcript Sweep

## Problem
1. The project's documentation (`ARCHITECTURE.md` and `DEVELOPMENT.md`) is out-of-sync with the massive changes implemented in Mythrax 2.0.
2. Trailing transcript context from agent executions can be lost if a session is abandoned before its next compaction cycle, as the transcript was not mined into the database.

## Outcome
1. Completely up-to-date, accurate, and step-by-step system architecture and data flow documentation in the repository root.
2. A robust, automated background transcript sweep inside the dreaming compactor that detects abandoned sessions, mines their final trailing turns into the database, and processes them in the dreaming summaries.

## User Value
* **System Observability**: Developers can rely on codebase documentation to understand all operational flows and testing protocols.
* **Context Preservation**: Prevents loss of final trailing executions and tool outputs when agent sessions are aborted or abandoned.

## In Scope
1. Overwriting `ARCHITECTURE.md` and `DEVELOPMENT.md` in the repository root with up-to-date mappings.
2. Stashing `_transcript_path` in STM upon session startup/initialization.
3. Querying all registered transcript paths in `synthesis.rs` during dreaming:
   `SELECT session_id, value AS path FROM short_term_memory WHERE key = '_transcript_path';`
4. Evaluating session idleness (e.g. no STM changes for $>10$ minutes).
5. Invoking `mine_transcript(session_id, path)` for idle sessions in the background.
6. Cleaning up the `_transcript_path` key in STM on successful sweep.
7. Adding a regression integration test in `test_compactor.rs` to verify the background sweep works.

## Out of Scope
* Automatic directory scanning for unregistered or untracked transcript files.
* Compacting or mining files that are not registered in the STM registry.

## Inputs
* STM record: `short_term_memory:['session_id', '_transcript_path']` with the path to the transcript file.
* Local JSONL transcript file.

## Outputs
* Mined episodes saved in SurrealDB.
* Historical digests archived to `~/.mythrax/archive/`.

## Constraints
* The background sweep must run prior to retrieving unprocessed episodes in `run_dream` to ensure swept episodes are included in the active dreaming cycle.
* The sweep must be non-blocking for subsequent sessions (safely skip files that are locked or missing).

## Risks and Edge Cases
* **Missing/Deleted Transcript File**: The session was deleted or the temp directory was cleaned up. *Mitigation*: The sweep logs a warning and deletes the STM key to avoid infinite retry loops.
* **Active Session Sweep**: Sweeping a session that is still actively processing. *Mitigation*: Strictly enforce the 10-minute idle threshold based on the latest STM key update timestamp.

## Acceptance Criteria
- [ ] Overwritten `ARCHITECTURE.md` contains accurate step-by-step data flows for Bootstrapping, Watchers, completions proxies, model swaps, and compactors.
- [ ] Overwritten `DEVELOPMENT.md` contains the correct individual test commands for all these data flow steps.
- [ ] Stashing `_transcript_path` in STM registers the path successfully.
- [ ] Background dreaming coordinator queries STM for `_transcript_path` and mines idle sessions (>10 mins inactivity).
- [ ] STM cleanup deletes the `_transcript_path` key upon successful sweep completion.
- [ ] Integration tests in `test_compactor.rs` verify that an idle session is successfully swept and compacted.
