# Test Plan: Phase 4 Collaborative & Vault Lifecycle (v0.9.x)

This document outlines the test suite designed to verify **Phase 4: Collaborative & Vault Lifecycle** features. All tests will be implemented in `mythrax-core/tests/test_phase_4_lifecycle.rs`.

---

## 1. Unit & Integration Tests

### 1.1 Federated Promotion & Auto-Push (T1)
-   **Test Case**: `test_federated_promotion_and_auto_push`
-   **Verification**:
    1.  Create a local wisdom rule with utility = `55.0` (which is $\ge 50.0$) and scope = `testing`.
    2.  Save the rule using `SurrealBackend::save_wisdom_rule`.
    3.  Verify that the rule markdown file is copied to `<project_root>/.mythrax-shared/wisdom/proposed/`.
    4.  Verify that a git subprocess is spawned (mocked or run in a temp git repository).
    5.  Verify that the console outputs the alert:
        `[Mythrax Synapse: Auto-Promoted Wisdom Rule to GitHub -> committed as <hash>. To rollback, run: git revert <hash>]`.

### 1.2 Concatenated Conflict Resolution & Pre-Commit Hook (T2)
-   **Test Case**: `test_merge_vault_conflict_resolution`
-   **Verification**:
    1.  Create two conflicting rules in `.mythrax-shared/` with the same `target_pattern` but different remedies.
    2.  Run the merge logic programmatically (equivalent to `mythrax merge-vault`).
    3.  Verify that the two rules are merged into a single file under `.mythrax-shared/wisdom/proposed/` with their remedies, avoided actions, and explanations concatenated.
    4.  Verify that the warning block `> [!WARNING]` is prepended.
    5.  Verify that original conflicting files are moved to `.mythrax-shared/wisdom/conflict_archive/`.
-   **Test Case**: `test_pre_commit_hook_sanitization`
-   **Verification**:
    1.  Create a staged file in a temp git repo under `.mythrax-shared/` containing a secret (e.g., `api_key = "sk-12345"`).
    2.  Run the pre-commit sanitization logic (equivalent to `mythrax pre-commit`).
    3.  Verify that the secret is successfully redacted and the file is re-staged.

### 1.3 Biological Episode Decay (T3)
-   **Test Case**: `test_biological_episode_decay`
-   **Verification**:
    1.  Save an episode with utility = `50.0` and `last_retrieved_at` set to 10 days ago.
    2.  Perform a search query.
    3.  Verify that during retrieval, the returned utility score of the episode has decayed exponentially:
        $$U_{new} = 50.0 \times e^{-0.05 \times 10} \approx 30.32$$
    4.  Verify that the decayed score is used to compute the blended ranking score.
    5.  Perform another search where the episode is selected/cited.
    6.  Verify that the episode's utility score is reinforced back to `50.0` and its `last_retrieved_at` is reset to the current time.
    7.  Verify that updates are batched and written to the database asynchronously.

### 1.4 Cognitive Sleep & Archiving (T4)
-   **Test Case**: `test_cognitive_sleep_and_archiving`
-   **Verification**:
    1.  Create an episode with utility = `3.0` (which is $< 5.0$) and a corresponding physical file in the vault.
    2.  Run the sleep compaction routine.
    3.  Verify that the active record is deleted from SurrealDB.
    4.  Verify that the physical file is moved to `vault/archive/`.
    5.  Verify that a high-level Raptor summary of the episode is generated and saved as a wiki node.

### 1.5 Auditor Calibration & Footnote Citations (T5 & T6)
-   **Test Case**: `test_auditor_calibration_and_citations`
-   **Verification**:
    1.  Create a mock database with a search similarity threshold of `0.6`.
    2.  Run the auditor calibration routine.
    3.  Verify that it generates synthetic queries, performs searches, and adjusts the threshold in the config table if a target is missed.
    4.  Add episode IDs to the session STM under `_session_citations`.
    5.  Save a handoff file via `save_handoff`.
    6.  Verify that a formatted markdown footnote block listing clickable absolute Obsidian file paths of the cited episodes is automatically appended to the handoff file.
