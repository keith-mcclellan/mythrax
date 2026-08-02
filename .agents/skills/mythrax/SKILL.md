---
name: mythrax
description: Query memory via the MCP server before starting tasks, verify vault integrity, and run HTR loops.
---

# Mythrax Unified Memory, Integrity & Cognitive Guidance (v3.0.0)

The **Mythrax** MCP server provides semantic memory storage, retrieval, reinforcement, compliance verification, self-healing, cognitive hypothesis execution, short-term memory (STM), document ingestion via Forge, and workspace documentation vault mirroring.

---

## MCP Tools Reference & Detailed Guide

Granular legacy tools are consolidated into 4 action-based tools to reduce context schema bloat. You invoke them via the MCP server under names: `read`, `write`, `manage`, and `agent`.

### 1. `read` (Read-Only Operations)

Call the `read` tool with the `action` parameter set to one of the following:

- **`action="view"`**: Reads a text or source file (paging large blocks into virtual placeholders to save tokens).
  - *Parameters*: `path: String`, `start_line: Option<integer>`, `end_line: Option<integer>`, `is_skill_file: Option<boolean>`, `token_budget: Option<integer>`
- **`action="search"`**: Search episodic memories using 6-Signal Unified Retrieval (bounded by 50-item paginated windows).
  - *Parameters*: `query: String`, `scope: Option<String>`, `limit: Option<integer>`, `threshold: Option<number>`, `include_artifacts: Option<boolean>`, `include_episodes: Option<boolean>`
- **`action="rules"`**: Query active wisdom rules.
  - *Parameters*: `query: String`, `scope: Option<String>`
- **`action="nodes"`**: Hydrate specific node IDs.
  - *Parameters*: `node_ids: Vec<String>`
- **`action="root"`**: Get the absolute vault root path.
  - *Parameters*: None
- **`action="get"`**: Read stashed STM variables.
  - *Parameters*: `session_id: String`, `key: Option<String>`
- **`action="query_symbolic"`**: Query relation graphs (bounded by a global 1,000-hit cap with $O(1)$ hit indexing).
  - *Parameters*: `node_id: String`, `relation: Option<String>`, `max_depth: Option<integer>`
- **`action="search_index"`**: Fast index search for file lists or node IDs.
  - *Parameters*: `query: String`, `scope: Option<String>`, `limit: Option<integer>`
- **`action="timeline"`**: Chronological event query.
  - *Parameters*: `query: Option<String>`, `anchor_id: Option<String>`, `session_id: Option<String>`, `limit: Option<integer>`
- **`action="get_full"`**: Read raw, unpaged file contents or full unpaged memory nodes.
  - *Parameters*: `path: Option<String>`, `node_ids: Option<Vec<String>>`
- **`action="search_by_concept"`**: Retrieve memories matching a specific concept.
  - *Parameters*: `concept: String`
- **`action="diff_sessions"`**: Compare STM state between two sessions.
  - *Parameters*: `session_a: String`, `session_b: String`

---

### 2. `write` (Write & Mutation Operations)

Call the `write` tool with the `action` parameter set to one of the following:

- **`action="replace"`**: Surgically edit a single contiguous block in a file.
  - *Parameters*: `path: String` or `TargetFile: String`, `target_content: String` or `TargetContent: String`, `replacement_content: String` or `ReplacementContent: String`, `instruction: String`, `description: String`, `start_line: Option<integer>`, `end_line: Option<integer>`, `allow_multiple: Option<boolean>`
- **`action="multi_replace"`**: Apply non-contiguous edits across a file.
  - *Parameters*: `path: String` or `TargetFile: String`, `chunks: Vec<ReplacementChunk>`, `instruction: String`, `description: String`
- **`action="save"`**: Save a new episodic memory (computes SHA-256 `content_hash` for $O(1)$ indexed deduplication).
  - *Parameters*: `title: String`, `content: String`, `scope: Option<String>`, `node_type: Option<String>`, `session_id: Option<String>`, `duration: Option<String>`
- **`action="feedback"`**: Record reinforcement feedback for an episode.
  - *Parameters*: `episode_id: String`, `success: boolean`
- **`action="put"`**: Write a temporary variable to session STM.
  - *Parameters*: `session_id: String`, `key: String`, `value: String`
- **`action="clear"`**: Clear session STM.
  - *Parameters*: `session_id: String`
- **`action="handoff"`**: Register subagent delegation handoff contract.
  - *Parameters*: `parent_conversation_id: String`, `subagent_conversation_id: String`, `summary: String`, `handoff_file_path: String`, `scope: Option<String>`
- **`action="save_wisdom"`**: Save or update a WisdomRule node.
  - *Parameters*: `target_pattern: String`, `action_to_avoid: String`, `causal_explanation: String`, `prescribed_remedy: String`, `scope: Option<String>`, `tier: Option<String>`
- **`action="cognitive_callback"`**: Return LLM task results from background synthesis pipelines to SurrealDB.
  - *Parameters*: `callback_id: String`, `result: String`

---

### 3. `manage` (Workspace, Synthesis & Verification Tasks)

Call the `manage` tool with the `action` parameter set to one of the following:

- **`action="verify"`**: Verify link integrity and sync schemas across 700+ vault markdown files.
  - *Parameters*: `fix: Option<boolean>`
- **`action="organize"`**: Re-align directory structures and sync physical vault files with database index.
  - *Parameters*: None
- **`action="reprocess"`**: Re-index vault nodes and regenerate vector embeddings asynchronously in background.
  - *Parameters*: `reset_processed: Option<boolean>` (set `true` only for explicit full LLM fact re-extraction)
- **`action="sync_workspace"`**: Synchronize workspace documentation (`ARCHITECTURE.md`, `specs/`, `conductor/`) into human-readable reference nodes.
  - *Parameters*: `workspace_path: Option<String>`
- **`action="summarize"`**: Trigger manual compactions across memory scopes.
  - *Parameters*: `scope: String`, `async_mode: Option<boolean>`
- **`action="hypothesize"`**: Cluster unassociated facts and queue cognitive hypothesis formation tasks.
  - *Parameters*: `scope: Option<String>`
- **`action="refine"`**: Queue refinement tasks for pending idea nodes against supporting evidence.
  - *Parameters*: `scope: Option<String>`
- **`action="graduate"`**: Execute cross-scope wisdom graduation pipeline to promote universal claims into `WisdomRule` nodes.
  - *Parameters*: `scope: Option<String>`
- **`action="extract"`, `action="extract_code"`**: Extract atomic facts from documents or source code files.
  - *Parameters*: `doc_path: String` or `file_path: String`, `scope: Option<String>`
- **`action="complete_handoff"`**: Validate subagent handoff contract outputs, enforce enum and required field constraints, truncate STM values $> 32k$ chars, and mark handoff status as COMPLETED/FAILED.
  - *Parameters*: `task_id: String`, `status: Option<String>`, `outputs: Option<Object>`, `fail_reason: Option<String>`
- **`action="ingest_bulk"`**: Bulk ingest external agent log directories.
  - *Parameters*: `source: String`, `harness: String`, `scope: Option<String>`
- **`action="ingest_forge"`**: Ingest candidate documents (PDF/Markdown) via Forge pipeline.
  - *Parameters*: `source: String` or `source_path: String`, `scope: Option<String>`
- **`action="save_forged_assets"`**: Save rule documents and compactions.
  - *Parameters*: `doc_title: String`, `scope: String`, `chunk_index: integer`, `chunk_text: String`, `concepts: Vec<ForgedConcept>`, `rules: Vec<ForgedRule>`
- **`action="pre_invocation"`**: Load POMDP belief states and hydrate context before turn execution.
  - *Parameters*: `session_id: String`, `workspace_path: Option<String>`
- **`action="precompact"`**: Compact active transcripts into raw turn episodes.
  - *Parameters*: `session_id: String`, `transcript_path: Option<String>`
- **`action="audit_compliance"`**: Scan files against compliance rules and verify daemon health.
  - *Parameters*: `files: Option<Vec<String>>`, `workspace_path: Option<String>`
- **`action="clean"`**: Clean temporary build files and stale branches.
  - *Parameters*: `scope: Option<String>`, `confirm: Option<boolean>`
- **`action="bootstrap"`**: Run system bootstrapping.
  - *Parameters*: `scope: Option<String>`
- **`action="prune"`**: Prune stale memories across all 4 relation tables (`relates_to`, `followed_by`, `mentions`, `superseded_by`).
  - *Parameters*: `scope: Option<String>`
- **`action="tree_add_node"`, `action="tree_update_node"`, `action="tree_prune"`, `action="tree_view"`, `action="git_merge_branch"`**:
  - *Usage*: Manage Arbor hypothesis exploration tree state and worktree branches.
  - *Parameters*: `node_id: Option<String>`, `claim: Option<String>`, `confidence: Option<number>`
- **`action="init"`, `action="ideate"`, `action="execute"`, `action="backprop"`, `action="merge"`, `action="run"`**:
  - *Usage*: Execute HTR (Hypothesize-Test-Refine) loop stages across isolated git worktrees.
  - *Parameters*: `hypothesis: Option<String>`, `test_command: Option<String>`, `max_steps: Option<integer>`, `node_id: Option<String>`, `scope: String`

---

### 4. `agent` (Autonomous Subagent & Handoff Orchestration)

Call the `agent` tool with the `action` parameter set to one of the following:

- **`action="complete_code_task"`** (alias `complete_task`): Spawn an autonomous subagent loop to complete a coding chore.
  - *Parameters*: `prompt: String`, `system_instruction: Option<String>`, `model: Option<String>`, `enable_thinking: Option<boolean>`
- **`action="handoff"`** (alias `save_handoff`): Register a subagent delegation handoff contract in SurrealDB and link parent-child conversation context.
  - *Parameters*: `parent_conversation_id: String`, `subagent_conversation_id: String`, `summary: String`, `handoff_file_path: String`, `scope: Option<String>`

---

## Workspace & Project Documentation Vault Mirroring

Mythrax automatically mirrors workspace-root and Conductor documentation assets (`ARCHITECTURE.md`, `REINITIALIZATION.md`, `conductor/tracks/**/*.md`, `specs/**/*.md`) into the human-readable vault (`vault_root/reference/`) via `sync_workspace_docs_to_vault`:
* **Path Normalization**: Preserves relative directory hierarchies (`specs/arbor_htr/test-plan.md` -> `vault_root/reference/specs/arbor_htr/test-plan.md`) with cross-platform forward-slash (`/`) path normalization.
* **SHA-256 Diffing**: Compares structural SHA-256 hashes to skip unchanged files without disk re-writes or DB queries.
* **Lightweight Reference Indexing**: Indexes reference chunks directly into `WikiNode` records (`node_type: "reference"`, `scope: "workspace_ref"`, `name: relative/path.md#part-N`) without LLM extraction loops.
* **Atomic MOC Rebuilding**: Surgically updates `## Reference` in `MOC.md` via atomic `.tmp` file swaps.
* **Atomic Cascade Purging**: Deletion pruning executes in single atomic transaction blocks cascading all 4 relation tables (`relates_to`, `followed_by`, `mentions`, `superseded_by`) and `metrics` records.

---

## Pre-Invocation Hook & Verification Compliance

1. **Automatic Context Injection**: The system runs `pre_invocation` automatically before your first turn. It injects active POMDP belief states, stashed STM variables, handoff tasks, and three-tier hybrid hydration memory nodes:
   - **Similarity >= 0.80**: Hydrated fully.
   - **Similarity [0.60, 0.80)**: Listed in summary tables.
   - **Similarity < 0.60**: Discarded.
2. **Policy vs. Advisory Separation**: Context injection strictly segregates high-importance P0 binding rules (Policy) from P1 suggestions (Advisory):
   - **Policy Section**: Rendered first, using warning callouts (`> [!CAUTION]`) to enforce critical constraints (e.g. GPU timeouts, path safety).
   - **Advisory Section**: Rendered second, using tip callouts (`> [!TIP]`) for optional guidance and performance suggestions.
3. **Boot Verification**: You **MUST** output compliance verification on the first line of your first response:
   `Execution Check: [Karpathy Rules applied? Yes/No] [Local Model verified? Yes/No/Fallback]`
4. **Enforced Memory Search**: If the pre-invocation context is empty, manually run `read(action="search", query="...")` before editing code.
5. **Reinforcement**: Run `write(action="save")` to log results and `write(action="feedback")` to reinforce the pathway.

---

## Agent Handoff Protocol

When delegating tasks:
1. Discover the vault root via `read(action="root")`.
2. Write the contract file to `<vault_root>/.handoffs/handoff_<task_id>.md`.
3. **Typed I/O Contracts**: The handoff system enforces strict YAML-based contract validation on boundaries:
   - **`write(action="handoff")`**: Parses the handoff contract's input parameters, validating types, requirements, and allowed enum values before spawning the subagent. Input values are safely logged to the subagent's STM.
   - **`complete_handoff`**: Validates the subagent's final output outputs, formats status strings using regex filters, and promotes output values to the parent session's STM.
4. Save the distilled context node IDs in STM under key `"distilled_context_nodes"`.
5. Call `write(action="handoff", ...)` to link parent and child nodes in SurrealDB.
6. Spawn the subagent pointing to the contract path:
   > *"Read and execute the handoff at `file:///<vault_root>/.handoffs/handoff_<task_id>.md` and rules at `file:///Users/keith/.gemini/AGENT.md`. Output first: `Execution Check: [Karpathy Rules applied? Yes/No] [Local Model verified? Yes/No/Fallback]`"*

---

## Virtual Paging & Editing

To fit large codebases into context windows:
1. **Virtual Skeletons**: `read(action="view")` returns code with placeholders (e.g. `[Paged Symbol: ...]`) instead of full bodies. Disk files remain untouched.
2. **Paging-Aware Edits**: `write(action="replace")` and `write(action="multi_replace")` parse placeholders, query `symbol_archive` to restore bodies in memory, apply the replacement, and write back to disk. Target placeholders exactly as they appear in the skeleton.
3. **LRU Eviction**: Unused memories are evicted from RAM. Wisdom rules, high importance nodes ($\ge 8.0$), active handoffs, and active STM are pinned.

---

## Post-Task Reflect Hook

To consolidate lessons and prevent forgetting loops:
1. **Asynchronous Reflection**: The daemon periodically harvests finished sessions and triggers a `reflection_distillation` task.
2. **Cognitive Ingestion**:
   - Successful tasks are distilled into experience episodes (`node_type: "experience"`) to reinforce similar future queries.
   - Failed tasks undergo contrastive analysis to identify contradictions, generating new pruned hypotheses or updating/reinforcing existing wisdom rules.
3. **Dreaming Exclusion**: Experience episodes are strictly excluded from dreaming compactions to preserve raw execution trajectories and prevent semantic hallucination.
