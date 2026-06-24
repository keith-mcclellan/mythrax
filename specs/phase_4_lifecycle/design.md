# Design: Phase 4 Collaborative & Vault Lifecycle (v0.9.x)

This document describes the technical design for implementing **Phase 4: Collaborative & Vault Lifecycle** in Project Mythrax.

---

## 1. Federated Promotion & Auto-Push (T1)

### Promotion Logic
1.  **Trigger**: Inside `SurrealBackend::save_wisdom_rule`, after a rule has been successfully committed to SurrealDB:
    *   Check if `rule.utility >= Some(50.0)`.
    *   Check if `rule.scope != "general"`.
2.  **Resolution**:
    *   Read the environment variable `MYTHRAX_WORKSPACE_ROOT`.
    *   If not set, default to the current working directory.
    *   Construct the target directory path: `<project_root>/.mythrax-shared/wisdom/proposed/`.
    *   Ensure the target directory exists (`std::fs::create_dir_all`).
3.  **File Copying**:
    *   If the rule has a `vault_path` (which points to the local markdown file relative to the vault root):
        *   Resolve the absolute source path using `find_vault_root().join(&vp)`.
        *   Determine the filename (e.g., `rule-name.md`).
        *   Construct the destination path: `<project_root>/.mythrax-shared/wisdom/proposed/<filename>`.
        *   Copy the file using `std::fs::copy`.
4.  **Auto-Push Command**:
    *   Spawn a background OS thread (`std::thread::spawn`) to execute the git commands, ensuring zero latency impact on the user or the API call:
        ```bash
        cd <project_root> && git add .mythrax-shared/wisdom/proposed/<filename> && git commit -m "mythrax: auto-promote wisdom rule" && git push
        ```
    *   Capture the output of `git rev-parse --short HEAD` after committing to get the `<hash>`.
    *   Display a prominent alert in the console:
        `[Mythrax Synapse: Auto-Promoted Wisdom Rule to GitHub -> committed as <hash>. To rollback, run: git revert <hash>]`

---

## 2. Conflict Resolution & Pre-Commit Hook (T2)

### 2.1 CLI Subcommand `merge-vault`
1.  **Scanning**:
    *   Scan `.mythrax-shared/` recursively for `.md` files.
    *   For each file, parse the frontmatter and content using `parse_frontmatter`.
    *   Filter for rules (files with frontmatter containing `target_pattern`).
2.  **Grouping**:
    *   Group rules by `target_pattern`.
3.  **Concatenation & Deduplication**:
    *   If multiple rules exist for the same `target_pattern`:
        *   Combine their frontmatter fields:
            *   `target_pattern`: original pattern.
            *   `action_to_avoid`: concatenate with a bulleted list or newline.
            *   `causal_explanation`: concatenate with a bulleted list or newline.
            *   `prescribed_remedy`: concatenate with a bulleted list or newline.
            *   `tier`: use the highest tier (e.g., "skills" > "wisdom" > "dynamic").
            *   `scope`: if different, use "general" or combine.
            *   `utility`: use the maximum utility score among them.
        *   Construct the unified markdown file:
            *   Prepend a `> [!WARNING]` block:
                ```markdown
                > [!WARNING]
                > This rule was automatically merged from conflicting duplicates. Please review and edit manually.
                ```
            *   Write the frontmatter and content to `.mythrax-shared/wisdom/proposed/<slug>.md`.
        *   Move all original conflicting files to `.mythrax-shared/wisdom/conflict_archive/<filename>`.
        *   Log a warning alert:
            `[Conflict Resolution: Rules merged for pattern '<pattern>'. Original files archived under .mythrax-shared/wisdom/conflict_archive/]`

### 2.2 CLI Subcommand `install-hook`
1.  **Script Writing**:
    *   Write a pre-commit shell script to `.git/hooks/pre-commit`:
        ```bash
        #!/bin/sh
        # Mythrax pre-commit hook to clean secrets
        exec mythrax pre-commit
        ```
    *   Make the hook file executable (`chmod +x`).

### 2.3 CLI Subcommand `pre-commit`
1.  **Staged Files Scan**:
    *   Run `git diff --cached --name-only --diff-filter=ACM` to get all staged file paths.
    *   Filter paths that start with `.mythrax-shared/`.
2.  **Sanitization**:
    *   For each matching file:
        *   Read the content.
        *   Clean the content using `SecretFilter::clean`.
        *   Write the sanitized content back to the file.
        *   Re-add the file to git: `git add <file>`.

---

## 3. Biological Decay Loop (T3)

### Schema Updates
1.  **Contracts**:
    *   Add `last_retrieved_at: Option<String>` and `utility: Option<f32>` to `Episode` and `EpisodeRaw`.
2.  **Initialization**:
    *   When an episode is created via `save_episode`, initialize `utility = Some(50.0)` and `last_retrieved_at = Some(chrono::Utc::now().to_rfc3339())`.

### On-the-fly Decay & Reinforcement
1.  **Decay Calculation**:
    *   During `search` in `backend.rs`, for every retrieved episode `ep`:
        *   Read $U_{old}$ (defaulting to `50.0` if None).
        *   Read `last_retrieved_at` timestamp. If None, default $\Delta t = 0$.
        *   Calculate the elapsed time $\Delta t$ in days:
            $$\Delta t = \frac{\text{now} - \text{last\_retrieved\_at}}{86400\text{ seconds}}$$
        *   Calculate the decayed score:
            $$U_{new} = U_{old} \times e^{-\lambda \Delta t}$$
            where $\lambda = 0.05$ (default, configurable in `config.json` via a `decay_rate` field or defaults to `0.05`).
        *   Use $U_{new}$ for the blended score ranking calculation.
2.  **Batched Background Write**:
    *   We maintain an in-memory queue of pending utility updates: `Arc<Mutex<HashMap<String, (f32, String)>>>`.
    *   During the search, the decayed scores are queued.
    *   If an episode is **selected (cited)**:
        *   Reinforce it: set utility to `50.0` and `last_retrieved_at = current_timestamp` in the queue, overwriting any decayed values.
    *   At the end of the session (or asynchronously every few minutes via a background task), write the batched updates to SurrealDB.

---

## 4. Cognitive Sleep & Archiving (T4)

### Compaction Loop
1.  **Trigger**: Inside `compactor.rs` during daily compaction (`compact_scope` and `compact_global`).
2.  **Identification**:
    *   Query SurrealDB for all episodes with `utility < 5.0`.
3.  **Archiving Steps**:
    *   For each decayed episode:
        1.  If the physical markdown file exists (resolved via `vault_path`), move the file from its current location to `vault/archive/<filename>`.
        2.  Strip its vector embedding in SurrealDB (or delete the record).
        3.  Generate a high-level Raptor summary of the episode using the LLM.
        4.  Save the Raptor summary as a wiki node, preserving the historical trace in a highly compressed form.
        5.  Delete the active `episode` record from SurrealDB.

---

## 5. Auditor Self-Healing & Citation Footnotes (T5 & T6)

### 5.1 Auditor Daemon
1.  **Trigger**: Runs every 24 hours in the background daemon, or manually via `mythrax audit`.
2.  **Calibration Loop**:
    *   Select 3-5 existing episodes from SurrealDB at random.
    *   For each episode:
        *   Use the LLM to generate a synthetic query based on the episode's title and content.
        *   Perform a semantic vector search using `search` with the synthetic query.
        *   Verify if the source episode is returned in the top $N$ results.
        *   If the source episode is **missed** (its similarity score is below the current threshold):
            *   Adjust the similarity threshold dynamically in the database configuration table: decrease the threshold slightly (e.g., by `0.05`) to widen search sensitivity.
            *   If it returns too many false positives, increase the threshold slightly.
            *   Save the adjusted threshold to the configuration table.

### 5.2 Citations & Footnotes
1.  **Citations Tracking**:
    *   Maintain a list of cited episode IDs in the session STM under the key `_session_citations`.
    *   Whenever `search_memories` is called, the MCP server parses the results and can record retrieved episode IDs, or the agent explicitly adds cited IDs to `_session_citations` via `put_short_term`.
2.  **Footnote Insertion**:
    *   When the agent saves a handoff file via `save_handoff`, or when the daemon/CLI generates a task plan or git commit message:
        *   Retrieve the list of cited episode IDs from the session STM.
        *   Look up their details (title and `vault_path`) in SurrealDB.
        *   Format a `Citations` markdown footnote block:
            ```markdown
            ### Citations
            - [Episode Title](file:///Users/keith/mythrax-vault/episodes/2026-06-24-episode-name.md)
            ```
        *   Append this block to the end of the handoff/plan file or commit message.
