# Design: Meta-Skill Synthesis

## Overview
Meta-Skill Synthesis will be implemented as a new cognitive module in `mythrax-core/src/cognitive/meta_skill.rs`. It will read `WisdomRule`s and `WikiNode`s from SurrealDB, cluster them semantically using DBSCAN, and call the LLM to synthesize targeted playbooks (SKILL.md) written to `.agents/skills/<skill-name>/SKILL.md`.

## Data Model & Query Additions
We will add `get_all_wiki_nodes` to the `StorageBackend` trait and the `SurrealBackend` struct:
```rust
async fn get_all_wiki_nodes(&self) -> Result<Vec<WikiNode>>;
```

## Execution Flow
1. **Retrieve Data**:
   - Query all active scopes: `db.get_active_scopes().await?`.
   - Query all wisdom rules: `db.get_all_wisdom_rules().await?`.
   - Query all wiki nodes: `db.get_all_wiki_nodes().await?`.
   - Scan global and project skill directories (`~/.gemini/config/skills` and `.agents/skills`) to load existing playbooks.
2. **Group by Scope**:
   - Instead of clustering across all records arbitrarily, we group all rules, wiki nodes, and existing playbooks by their `scope` field (e.g. `"surrealdb"`, `"arbor"`, `"general"`, `"git"`).
   - If an item doesn't have a scope, or if its scope is empty, it is grouped under the `"general"` scope.
3. **Targeted Scope Synthesis**:
   - For each active scope (e.g., `"surrealdb"`):
     - Gather all wisdom rules, wiki nodes, and existing playbooks belonging to that scope.
     - If the combined text size exceeds a safe threshold (e.g., 50,000 characters), run DBSCAN clustering *only* within this scope to split it into sub-categories, or summarize them to stay token-conscious.
     - If the scope contains at least one wisdom rule or wiki node, we synthesize a single consolidated playbook: `.agents/skills/meta-<scope>/SKILL.md`.
4. **LLM Synthesis (Token-Conscious)**:
   - For each active scope containing data:
     - Format a prompt containing all scoped wisdom rules (target patterns, actions to avoid, remedies), wiki nodes (title, body), and existing playbooks.
     - Call the LLM with a system prompt instructing it to synthesize a single, unified `SKILL.md` for this scope.
     - Prompt:
       ```
       You are a meta-skill synthesizer. Analyze the following wisdom rules, playbooks, and forged document sections to generate a cohesive agent playbook.
       To maintain token consciousness and prevent context window bloat:
       1. Keep the generated SKILL.md file lightweight (e.g., under 400 lines) by summarizing core principles rather than reproducing large reference documents or every single wisdom rule verbatim.
       2. Explicitly instruct the AI agent in the playbook instructions to query the Mythrax memory engine using vector search tools (`search_wisdom`, `search_memories`) for detailed guidelines and forged artifact contents matching specific keywords (provide a list of suggested queries).
       3. Output ONLY the SKILL.md content starting with YAML frontmatter containing:
       ---
       name: meta-<scope-name>
       description: <active-voice-summary-of-this-scope-rules>
       ---
       Followed by the structured markdown instructions.
       ```
5. **Publish / Write to Disk**:
   - Write/overwrite the file to `.agents/skills/meta-<scope-name>/SKILL.md` under the repository root.
   - If parent directories do not exist, create them.
   - **Registration**: Because `.agents/skills/` is the standard project customization root recognized by the agent harness, placing playbooks here makes them immediately discoverable and dynamically loaded by the agent on subsequent runs without any manual registration or updates to `skills.json`.

## MCP Tool Integration
Expose this capability as a new MCP tool `synthesize_meta_skills` in `mythrax-core/src/mcp.rs`.
The tool accepts no arguments (or an optional `scope` to filter results).
When triggered, it runs the pipeline and returns a summary of the published skills.

## Playbook Maintenance & Update Strategy

To update and maintain playbooks safely without destroying bespoke human modifications:
1. **Ownership Tagging**:
   - Synthesized playbooks will include `generator_name: MetaSkillSynthesizer` in their YAML frontmatter.
   - Folder paths for synthesized skills will be prefixed with `meta-` (e.g. `.agents/skills/meta-git-workflow/`).
2. **Read-Only / Modification Guardrails**:
   - During the scan phase, any local or global skill folder that **does not** contain `generator_name: MetaSkillSynthesizer` in its frontmatter is classified as a **custom/manual skill**.
   - These manual skills are read as inputs for semantic clustering (providing context to the LLM), but they are **never overwritten**.
   - If a manual skill clusters with rules/docs, the synthesizer creates a companion meta playbook (e.g. `meta-<manual-skill-name>`) to capture the extensions, or prompts the LLM to update only the `meta-` prefixed playbooks.
3. **Dreaming Integration**:
   - The meta-skill synthesis pipeline can be triggered automatically at the end of the compactor/dreaming loop (`summarize_episodes`), ensuring that as new episodes are ingested and wisdom rules are dynamically generated/compacted, the corresponding meta-skills are refreshed.

## Triggered Skill Merging & Opportunity Detection

To support consolidating highly related skills and preventing fragmentation:

### 1. Automatic Opportunity Detection
- **Vector Cosine Similarity Check**:
  - The synthesizer scans all active custom and meta playbooks on disk and calculates the pairwise cosine similarity of their description embeddings.
  - If the similarity score between any two playbooks exceeds a high threshold (e.g. `similarity > 0.85`), they are marked as a candidate pair.
  - For candidate pairs, we run a short, token-conscious LLM call providing the names and descriptions of the candidate skills.
  - The LLM validates if they are indeed redundant/overlapping and outputs a JSON suggestion:
    ```json
    {
      "should_merge": true,
      "suggested_name": "git-workflow",
      "reason": "Both playbooks cover Git commands (pull vs commit) and should be merged."
    }
    ```
  - **Suggestions File**: The synthesizer writes/updates a dedicated markdown file `wiki/skill_merge_suggestions.md` in the vault root, listing all currently detected merge candidate playbooks, suggested target names, similarity scores, and reasoning.
- **Exposure**:
  - Expose this detection via a new MCP tool `detect_skill_merges` which scans the folders, updates the suggestions file, and returns a list of consolidations.

### 2. Triggered Skill Merge Execution
- **Method `merge_skills(db, store, source_skills: &[String], target_name: &str)`**:
  - Loads the full contents of all specified `source_skills` playbooks.
  - Formulates a synthesis prompt to the LLM to merge them into a single, cohesive playbook `meta-<target-name>/SKILL.md`.
  - Writes/publishes the consolidated playbook to the target folder.
  - **Cleanup & Archiving**:
    - **Meta Playbooks**: If a source skill is an auto-generated playbook (contains `generator_name: MetaSkillSynthesizer`), its directory is moved into the workspace `.trash/` folder (with a timestamp appended, e.g. `.trash/meta-git-commit_2026-06-23-11-03/`) to allow for manual recovery while keeping the active workspace clean.
    - **Custom Playbooks**: If a source skill is custom (lacks the synthesizer metadata tag), its directory is moved into `.agents/archive/skills/` (creating it if needed) so that the agent harness ignores it while preserving the user's manual modifications.
- **Exposure**:
  - Expose this execution via a new MCP tool `merge_skills` accepting a JSON array of `source_skills` and a `target_name`.
