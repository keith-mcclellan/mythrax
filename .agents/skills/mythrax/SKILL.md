---
name: mythrax
description: Query memory via the MCP server before starting tasks, verify vault integrity, and run HTR loops.
---

# Mythrax Unified Memory, Integrity & Cognitive Guidance (v3.0.0)

The **Mythrax** MCP server provides semantic memory storage, retrieval, reinforcement, compliance verification, self-healing, cognitive hypothesis execution, short-term memory (STM), and document ingestion via Forge.

---

## MCP Tools Reference & Detailed Guide

Granular legacy tools are consolidated into 4 action-based tools to reduce context schema bloat.

### 1. `read` (Read-Only Operations)

- **`view_file`**: Reads text or source files.
  - *Parameters*: `path: String`, `start_line: Option<u32>`, `end_line: Option<u32>`
  - *Behavior*: Automatically pages out large blocks into virtual placeholders (`[Paged Symbol: ...]`) to save tokens. Does not modify disk files.
  - *Usage*: Use to inspect files before editing.
- **`search_memory`**: Search episodic memories using 6-Signal Unified Retrieval.
  - *Parameters*: `query: String`, `scope: Option<String>`, `limit: Option<u32>`
  - *Usage*: Use to locate past tasks, solutions, and decisions.
- **`search_wisdom`**: Query active wisdom rules.
  - *Parameters*: `query: String`, `scope: Option<String>`
  - *Usage*: Use to research directory-specific constraints or coding style policies.
- **`get_memory_nodes`**: Hydrate specific node IDs.
  - *Parameters*: `node_ids: Vec<String>`
  - *Usage*: Use to retrieve full data structures of nodes passed in handoffs or STM.
- **`get_vault_root`**: Get absolute vault root path.
  - *Parameters*: None
  - *Usage*: Use to locate handoffs, compactions, and wiki directories.
- **`get_short_term`**: Read stashed STM variables.
  - *Parameters*: `session_id: String`, `key: Option<String>`
  - *Usage*: Use during boot to read context nodes or session variables.
- **`get_config`**: Fetch system settings.
  - *Parameters*: None
  - *Usage*: Use to inspect models, thresholds, or API keys.
- **`query_symbolic`**: Query relation graphs.
  - *Parameters*: `node_id: String`, `relation: Option<String>`, `max_depth: Option<u32>`
  - *Usage*: Use to traverse concept maps or task sequences.
- **`search_index`**: Fast index search.
  - *Parameters*: `query: String`, `scope: Option<String>`, `limit: Option<u32>`
  - *Usage*: Use to find file lists or node IDs while conserving tokens.
- **`timeline`**: Chronological event query.
  - *Parameters*: `session_id: Option<String>`, `limit: Option<u32>`
  - *Usage*: Use to review historical task sequences.
- **`get_full`**: Read raw, unpaged file contents.
  - *Parameters*: `path: String`
  - *Usage*: Use only for small config files or headers. Avoid on large source code.

---

### 2. `write` (Write & Mutation Operations)

- **`edit_file`**: Surgically edit a single contiguous block.
  - *Parameters*: `path: String`, `target_content: String`, `replacement_content: String`, `instruction: String`, `description: String`
  - *Behavior*: Reconstructs paged symbol placeholders in memory, applies modifications, and writes to disk.
  - *Usage*: Use for targeted single-block fixes.
- **`multi_edit_file`**: Apply non-contiguous edits.
  - *Parameters*: `path: String`, `chunks: Vec<ReplacementChunk>`, `instruction: String`, `description: String`
  - *Usage*: Use to modify multiple independent methods or blocks in one file.
- **`save_episode`**: Save a new episodic memory.
  - *Parameters*: `title: String`, `content: String`, `scope: Option<String>`
  - *Usage*: Use to log completed tasks, findings, and decisions.
- **`record_feedback`**: Record reinforcement feedback.
  - *Parameters*: `episode_id: String`, `success: bool`
  - *Usage*: Use after tests pass/fail to reinforce the memory pathway.
- **`put_short_term`**: Write temporary variable to STM.
  - *Parameters*: `session_id: String`, `key: String`, `value: String`
  - *Usage*: Use to share context or node IDs before spawning subagents.
- **`clear_short_term`**: Clear session STM.
  - *Parameters*: `session_id: String`
  - *Usage*: Use during teardown to clean up temporary state.
- **`save_forged_assets`**: Bulk write rule documents and compactions.
  - *Parameters*: `scope: String`, `chunks: Vec<Value>`
  - *Usage*: Internal compactor pipeline writes.
- **`ingest_bulk`**: Bulk ingest directories or files.
  - *Parameters*: `paths: Vec<String>`, `scope: String`
  - *Usage*: Use to index new vault or code directories.
- **`ingest_forge`**: Ingest candidate wisdom rules.
  - *Parameters*: `path: String`, `scope: String`
  - *Usage*: Use to graduate rules.
- **`set_config`**: Set daemon configuration.
  - *Parameters*: `key: String`, `value: String`
  - *Usage*: Use to adjust thresholds or API keys.

---

### 3. `manage` (Workspace & Verification Tasks)

- **`pre_invocation`**: Load context and belief states.
  - *Parameters*: `session_id: String`
  - *Usage*: Executed automatically. Manually call to refresh state.
- **`precompact`**: Compact active transcripts.
  - *Parameters*: `session_id: String`, `transcript_path: String`
  - *Usage*: Use to compress conversation logs before rule distillation.
- **`verify_vault`**: Verify link integrity and sync schemas.
  - *Parameters*: `fix: Option<bool>`
  - *Usage*: Use to self-heal link mappings and update DB tables.
- **`organize_vault`**: Re-align directory structures.
  - *Parameters*: None
  - *Usage*: Use to clean up folder structures.
- **`reprocess_vault`**: Re-index all vault nodes.
  - *Parameters*: None
  - *Usage*: Use to regenerate embeddings and re-chunk files.
- **`summarize_vault`**: Trigger compactions.
  - *Parameters*: `scope: String`
  - *Usage*: Use to manually start background dreaming loops.
- **`audit_compliance`**: Scan codebase against rules.
  - *Parameters*: `files: Vec<String>`
  - *Usage*: Use to identify compliance violations.
- **HTR Actions (`init_htr`, `ideate_htr`, `execute_htr`, `backprop_htr`, `merge_htr`, `run_htr`)**:
  - *Parameters*: `hypothesis: String`, `test_command: String`, `max_steps: Option<u32>`, etc.
  - *Usage*: Use to execute Hypothesize-Test-Refine cognitive loops.

---

### 4. `agent` (Agent Orchestration & Handoffs)

- **`complete_task`**: Spawn an autonomous subagent loop.
  - *Parameters*: `prompt: String`, `files: Vec<String>`, `model: Option<String>`, `system_instruction: Option<String>`
  - *Usage*: Use to delegate self-contained tasks asynchronously.
- **`save_handoff`**: Register delegation handoff.
  - *Parameters*: `parent_conversation_id: String`, `subagent_conversation_id: String`, `summary: String`, `handoff_file_path: String`
  - *Usage*: Use when spawning subagents to link parent-child context nodes in the graph.

---

## Pre-Invocation Hook & Verification Compliance

1. **Automatic Context Injection**: The system runs `pre_invocation_hook` before your first turn. It injects the active POMDP belief state, stashed STM variables, handoff tasks, and three-tier hybrid hydration memory nodes:
   - **Similarity >= 0.80**: Hydrated fully.
   - **Similarity [0.60, 0.80)**: Listed in summary tables.
   - **Similarity < 0.60**: Discarded.
2. **Boot Verification**: You **MUST** output compliance verification on the first line of your first response:
   `Execution Check: [Karpathy Rules applied? Yes/No] [Local Model verified? Yes/No/Fallback]`
3. **Enforced Memory Search**: If the pre-invocation context is empty, manually run `read(action="search_memory", query="...")` before editing code.
4. **Reinforcement**: Run `write(action="save_episode")` to log results and `write(action="record_feedback")` to reinforce the pathway.

### 6-Signal Unified Retrieval Pipeline
The system scores memory candidate retrieval using six signals: vector similarity, BM25, concept spreading activation, active STM memory injection (using `embed_batch` to avoid sequential embedding calls), temporal neighbors, and Gaussian time decay.

---

## Agent Handoff Protocol

When delegating tasks:
1. Discover the vault root via `read(action="get_vault_root")`.
2. Write the contract file to `<vault_root>/.handoffs/handoff_<task_id>.md`.
3. Save the distilled context node IDs in STM under key `"distilled_context_nodes"`.
4. Call `agent(action="save_handoff", ...)` to link nodes in SurrealDB.
5. Spawn the subagent pointing to the contract path:
   > *"Read and execute the handoff at `file:///<vault_root>/.handoffs/handoff_<task_id>.md` and rules at `file:///Users/keith/.gemini/AGENT.md`. Output first: `Execution Check: [Karpathy Rules applied? Yes/No] [Local Model verified? Yes/No/Fallback]`"*

---

## Virtual Paging & Editing

To fit large codebases into context windows:
1. **Virtual Skeletons**: `read(action="view_file")` returns code with placeholders (e.g. `[Paged Symbol: ...]`) instead of full bodies. Disk files remain untouched.
2. **Paging-Aware Edits**: `write(action="edit_file")` and `write(action="multi_edit_file")` parse placeholders, query `symbol_archive` to restore bodies in memory, apply the replacement, and write back to disk. Target placeholders exactly as they appear in the skeleton.
3. **LRU Eviction**: Unused memories are evicted from RAM. Wisdom rules, high importance nodes ($\ge 8.0$), active handoffs, and active STM are pinned.
