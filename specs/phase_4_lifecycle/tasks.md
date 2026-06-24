# Tasks: Phase 4 Collaborative & Vault Lifecycle (v0.9.x)

This document breaks down the implementation of Phase 4 into sequential, trackable tasks.

---

## T1: Federated Promotion & Auto-Push
-   **Purpose**: Automatically promote high-utility local rules to the shared team vault and push to GitHub.
-   **Related Requirements**: AC-4.1
-   **Related Tests**: `test_federated_promotion_and_auto_push`
-   **Inputs**: Local wisdom rule with utility $\ge 50.0$ saved in `save_wisdom_rule`.
-   **Actions**:
    1.  Update `save_wisdom_rule` in `backend.rs` to check for `utility >= 50.0` and `scope != "general"`.
    2.  Resolve `<project_root>` from the `MYTHRAX_WORKSPACE_ROOT` env var.
    3.  Create `.mythrax-shared/wisdom/proposed/` if it doesn't exist.
    4.  Copy the rule's markdown file to the shared folder.
    5.  Spawn a background OS thread to execute git stage, commit, and push, then print the revert instructions alert.
-   **Expected Output**: Rule file copied and pushed to git, warning alert printed to console.
-   **Validation**: Rule exists in the shared folder; git status/log shows the commit; alert printed.

---

## T2: Conflict Resolution & Pre-Commit Hook
-   **Purpose**: Implement CLI command to merge conflicting rules in the shared vault, and install/run a pre-commit secret sanitizer.
-   **Related Requirements**: AC-4.2
-   **Related Tests**: `test_merge_vault_conflict_resolution`, `test_pre_commit_hook_sanitization`
-   **Inputs**: Staged files in `.mythrax-shared/` and duplicate rules.
-   **Actions**:
    1.  Add `merge-vault`, `install-hook`, and `pre-commit` subcommands to `cli.rs` and match them in `main.rs`.
    2.  Implement `merge-vault` logic: recursively scan `.mythrax-shared/`, group by `target_pattern`, concatenate conflicting rules, prepending `> [!WARNING]`, and move originals to `conflict_archive/`.
    3.  Implement `install-hook` logic: write pre-commit script to `.git/hooks/pre-commit` and make it executable.
    4.  Implement `pre-commit` logic: scan staged files under `.mythrax-shared/`, run `SecretFilter::clean`, save and re-stage.
-   **Expected Output**: Working CLI commands for merging and secret sanitizing.
-   **Validation**: Execution of commands performs the specified operations successfully.

---

## T3: Biological Decay Loop
-   **Purpose**: Update episode schema and calculate exponential utility decay on-the-fly, reinforcing selected episodes.
-   **Related Requirements**: AC-4.3
-   **Related Tests**: `test_biological_episode_decay`
-   **Inputs**: Episode retrieval and selection/citation events.
-   **Actions**:
    1.  Add `last_retrieved_at` (timestamp) and `utility` (float) to `Episode` and `EpisodeRaw` in `contracts.rs` and `backend.rs`.
    2.  Implement exponential decay on-the-fly in `search` function:
        $$U_{new} = U_{old} \times e^{-\lambda \Delta t}$$
    3.  Implement reinforcing logic for cited episodes (reset utility to `50.0` and update `last_retrieved_at`).
    4.  Implement in-memory queue and batched asynchronous write for decayed utility scores.
-   **Expected Output**: Decayed scores used in search ranking; cited episodes reinforced; batched writes executed.
-   **Validation**: Retrieve unselected episode and verify decay; retrieve cited episode and verify reinforcement.

---

## T4: Cognitive Sleep & Archiving
-   **Purpose**: Identify decayed episodes, delete their DB records, strip embeddings, move files to archive, and generate Raptor summaries.
-   **Related Requirements**: AC-4.4
-   **Related Tests**: `test_cognitive_sleep_and_archiving`
-   **Inputs**: Daily compaction loop in `compactor.rs`.
-   **Actions**:
    1.  Query SurrealDB for episodes with `utility < 5.0` in `compactor.rs`.
    2.  Move their physical markdown files to `vault/archive/`.
    3.  Generate high-level Raptor summaries using the local LLM.
    4.  Create wiki nodes with the Raptor summaries.
    5.  Delete active SurrealDB records and strip embeddings.
-   **Expected Output**: Decayed episodes archived; database size reduced; summaries saved to wiki.
-   **Validation**: Compact vault and verify that files are archived and records deleted.

---

## T5 & T6: Auditor Calibration & Citations
-   **Purpose**: Calibrate similarity thresholds periodically using synthetic queries, and append footnote citations to task plans and commit messages.
-   **Related Requirements**: AC-4.5
-   **Related Tests**: `test_auditor_calibration_and_citations`
-   **Inputs**: Search results, session STM, and daemon execution.
-   **Actions**:
    1.  Add `audit` subcommand. In `main.rs`, implement Auditor Daemon selecting 3-5 episodes, generating synthetic queries, and adjusting search similarity thresholds.
    2.  In `mcp.rs`, track cited episode IDs under key `_session_citations` in session STM.
    3.  In `save_handoff` and `arbor.rs` commit messages, retrieve cited IDs and append the clickable footnote citations block pointing directly to Obsidian absolute file URIs.
-   **Expected Output**: Similarity thresholds calibrated; citation footnote blocks appended.
-   **Validation**: Verify threshold adjustments in DB config; check footnote formatting in handoff/commit outputs.

---

## T7: Verification & Testing
-   **Purpose**: Verify all changes by running the test suite.
-   **Related Requirements**: AC-4.1 - AC-4.5
-   **Related Tests**: `cargo test --test test_phase_4_lifecycle`
-   **Inputs**: Clean compilation of `mythrax-core`.
-   **Actions**:
    1.  Create `mythrax-core/tests/test_phase_4_lifecycle.rs` containing all unit and integration tests.
    2.  Run the tests and resolve any compiler warnings or test failures.
-   **Expected Output**: All tests pass cleanly.
-   **Validation**: `cargo test --test test_phase_4_lifecycle` output is green.
