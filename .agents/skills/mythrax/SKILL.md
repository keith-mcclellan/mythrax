---
name: mythrax
description: Always query Mythrax memory via the MCP server before starting tasks or making edits, verify vault/knowledge graph integrity, and run HTR cognitive execution loops.
---

# Mythrax Unified Memory, Integrity & Cognitive Guidance (v2.0)

You are equipped with the **Mythrax** MCP server, which exposes tools for semantic memory storage, retrieval, reinforcement, compliance verification, vault integrity self-healing, cognitive hypothesis execution, short-term memory sharing between agents, and document ingestion via Forge.

---

## MCP Tools Reference

All legacy granular tools have been consolidated into 4 high-efficiency, action-enum-based tools to reduce context schema bloat and prevent token overhead. Use these consolidated tools directly:

| Tool | Action Enum / Parameters | Purpose |
|------|---------------------------|---------|
| `read` | **Action**: `view_file` \| `search_memory` \| `search_wisdom` \| `get_memory_nodes` \| `get_vault_root` \| `get_short_term` \| `get_config` \| `query_symbolic` \| `search_index` \| `timeline` \| `get_full`<br>**Params**: `path?`, `start_line?`, `end_line?`, `query?`, `scope?`, `limit?`, `token_budget?`, `allow_downward?`, `node_ids?`, `node_id?`, `relation?`, `max_depth?`, `session_id?`, `key?`, `ids?` | Perform read-only operations including virtual page viewing (`view_file`), episodic memory search (`search_memory`), wisdom rules search (`search_wisdom`), hydrating node IDs (`get_memory_nodes`), retrieving vault root path (`get_vault_root`), retrieving short-term memory (`get_short_term`), fetching config (`get_config`), and timeline query (`timeline`). |
| `write` | **Action**: `edit_file` \| `multi_edit_file` \| `save_episode` \| `record_feedback` \| `put_short_term` \| `clear_short_term` \| `save_forged_assets` \| `ingest_bulk` \| `ingest_forge` \| `set_config`<br>**Params**: `path?`, `target_content?`, `replacement_content?`, `chunks?`, `allow_multiple?`, `instruction?`, `description?`, `title?`, `content?`, `scope?`, `episode_id?`, `success?`, `session_id?`, `key?`, `value?`, `provider?`, `duration?`, `model?`, `cloud_provider?`, `api_key?` | Perform write operations including contiguous file edits (`edit_file`), non-contiguous patch edits (`multi_edit_file`), saving episodic memories (`save_episode`), recording reinforcement feedback (`record_feedback`), setting short-term memory values (`put_short_term`), clearing short-term memory (`clear_short_term`), setting config (`set_config`), and ingesting assets (`save_forged_assets`, `ingest_bulk`, `ingest_forge`). |
| `manage` | **Action**: `pre_invocation` \| `precompact` \| `verify_vault` \| `organize_vault` \| `reprocess_vault` \| `summarize_vault` \| `audit_compliance` \| `init_htr` \| `ideate_htr` \| `execute_htr` \| `backprop_htr` \| `merge_htr` \| `run_htr`<br>**Params**: `session_id?`, `query?`, `workspace_path?`, `transcript_path?`, `fix?`, `scope?`, `hypothesis?`, `node_id?`, `files?`, `test_command?`, `max_steps?` | Perform management operations including workspace/compilation hook pre-invocation checking (`pre_invocation`), log/transcript precompacting (`precompact`), vault integrity verification (`verify_vault`), compliance auditing (`audit_compliance`), and cognitive HTR loop executions (`init_htr`, `ideate_htr`, `execute_htr`, etc.). |
| `agent` | **Action**: `complete_task` \| `save_handoff`<br>**Params**: `prompt?`, `files?`, `model?`, `system_instruction?`, `enable_thinking?`, `parent_conversation_id?`, `subagent_conversation_id?`, `summary?`, `handoff_file_path?`, `scope?`, `session_id?` | Perform agent-related coordination including executing an autonomous task loop (`complete_task`), and registering parent-subagent stm/obsidian handoffs (`save_handoff`). |

---

## Compliance & Memory Search Compliance (Pre-Invocation Hook)

To ensure high-quality execution and prevent duplicate coding effort:

1. **The Pre-Invocation Hook Runs Automatically**: Before your first turn, the system invokes `pre_invocation_hook`. This automatically injects:
   - **🧠 Active POMDP Belief State**: Injects the session's active belief state (tasks todo, hypotheses tested, confidence, uncertainty) at the top of the hook context to maintain cognitive continuity.
   - **⚡ Three-Tier Hybrid Hydration (Self-RAG)**:
     - **Similarity >= 0.80**: High-relevance nodes are fully hydrated (full content injected).
     - **Similarity in [0.60, 0.80)**: Mid-relevance nodes are indexed in a lightweight summary table (titles, scopes, and 1-sentence summaries) to conserve the token budget.
     - **Similarity < 0.60**: Low-relevance nodes are discarded.
   - **Omitted Symbol Restoration**: Automatic symbol restoration is omitted in the hook to keep disk files 100% clean and fully compiling.
   - Active handoff metadata and tasks from parent-to-subagent delegations.
   - Stashed session variables (Short Term Memory).
   - High-confidence memory nodes and HTR negative constraints.
2. **Boot Verification Hook**: You **MUST** read and verify the injected hook context on your very first turn and output your compliance check on the very first line of your response, formatted exactly as:
   `Execution Check: [Karpathy Rules applied? Yes/No] [Local Model verified? Yes/No/Fallback]`
3. **Enforced Memory Query**: If the pre-invocation hook output indicates no high-confidence memory episodes were found, or the query context is empty, you **MUST** manually invoke `read(action="search_memory", query="...")` with a specific search query related to your task *before* writing any code.
4. **Log Episodic Memory**: After completing a task, call `write(action="save_episode", title="...", content="...")` to persist your decisions and solutions.
5. **Record Feedback**: After running tests (e.g. `cargo test`) and verifying they pass, call `write(action="record_feedback", episode_id="...", success=true)` to reinforce retrieved wisdom.

---

## Agent Handoff Protocol & Smart Handoffs

When delegating work to a subagent, minimize context window usage and establish graph-linked memory associations:

### Spawning a Subagent

1. **Discover Vault Root**: Call `read(action="get_vault_root")` to obtain the Obsidian vault root path (e.g., `/Users/keith/mythrax-vault`).
2. **Write the contract** to `<vault_root>/.handoffs/handoff_<task_id>.md`.
3. **Link context and register handoff**:
   - Write active context node record IDs to STM under key `"distilled_context_nodes"` using `write(action="put_short_term", session_id="...", key="distilled_context_nodes", value="...")`.
   - Call `agent(action="save_handoff", ...)` to register the handoff, which automatically links it to the context nodes via `relates_to` edges in SurrealDB.
4. **Spawn the subagent** with a minimal prompt pointing to the vault handoff file:
   > *"Read and execute the handoff at `file:///<vault_root>/.handoffs/handoff_<task_id>.md` and rules at `file:///Users/keith/.gemini/AGENT.md`. Output first: `Execution Check: [Karpathy Rules applied? Yes/No] [Local Model verified? Yes/No/Fallback]`"*
5. **Hydrate context nodes**: The subagent reads the node IDs from STM (via `read(action="get_short_term")`) and calls `read(action="get_memory_nodes", node_ids=[...])` to hydrate all active context nodes in a single call.
6. **Lazy context via file URLs**: Reference large files as links (`[file.rs](file:///path#L50-L100)`) — never paste content.
7. **AST Symbol & Code Structure Targeting**:
   - Use line-scoped `read(action="view_file")` to read only the target AST symbols, preventing context window pollution from unrelated code.

---

## MemoryOS Paging & Paging-Aware Editing (v2.0)

To stay within LLM context windows and prevent token budget exhaustion while working with large codebases, Mythrax implements **MemoryOS Virtual Paging**:

### 1. Virtual In-Memory Skeletons (`read(action="view_file")`)
*   When you read a source code file (like `.rs`, `.py`, `.js`, `.ts`) using `read(action="view_file")`, the daemon automatically parses and pages out large symbol bodies (functions, structs, impls) to the `symbol_archive` SurrealDB table.
*   The tool returns a **virtual skeleton** containing lightweight placeholders (e.g. `[Paged Symbol: Reference page_fn_my_func]`) rather than the massive full-text bodies.
*   **Disk Integrity**: The physical file on disk is **never modified** during reads, keeping the codebase fully compiling.

### 2. Paging-Aware Editing (`write(action="edit_file")`)
*   Because you only see the virtual skeleton, your target edit blocks will naturally contain placeholders.
*   The editing tools (`write(action="edit_file")` and `write(action="multi_edit_file")`) are **paging-aware**: they scan your target block for placeholders, query `symbol_archive` to fetch the original bodies, reconstruct the unpaged target block in memory, find and replace it in the physical disk file, and save it.
*   **How to Edit**: Simply target the placeholders exactly as they appear in the skeleton. The daemon handles the reconstruction surgically.

### 3. LRU Eviction & Context Pinning
*   The daemon's `PagingManager` maintains an active context queue of memory nodes in RAM.
*   When capacity is reached, it automatically evicts the Least Recently Used (LRU) memories.
*   **Context Pinning**: Wisdom rules, high-importance items ($\ge 8.0$), active handoffs, and active STM are pinned in RAM and are **immune** to LRU eviction.
