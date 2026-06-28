---
name: mythrax
description: Always query Mythrax memory via the MCP server before starting tasks or making edits, verify vault/knowledge graph integrity, and run HTR cognitive execution loops.
---

# Mythrax Unified Memory, Integrity & Cognitive Guidance (v2.0)

You are equipped with the **Mythrax** MCP server, which exposes tools for semantic memory storage, retrieval, reinforcement, compliance verification, vault integrity self-healing, cognitive hypothesis execution, short-term memory sharing between agents, and document ingestion via Forge.

---

## MCP Tools Reference

All legacy granular tools (32 tools) have been consolidated into 8 high-efficiency, action-enum-based tools to reduce context schema bloat and prevent token overhead. Use these consolidated tools directly:

| Tool | Action Enum / Parameters | Purpose |
|------|---------------------------|---------|
| `manage_memory` | **Action**: `search` \| `rules` \| `nodes` \| `root` \| `query_symbolic` \| `save` \| `feedback` \| `thought` \| `search_index` \| `timeline` \| `get_full`<br>**Params**: `query?`, `scope?`, `limit?`, `token_budget?`, `allow_downward?`, `node_ids?`, `node_id?`, `relation?`, `max_depth?`, `title?`, `content?`, `entities?`, `vault_path?`, `episode_id?`, `success?` | Search memories (`search`), find wisdom rules (`rules`), hydrate node IDs (`nodes`), get the vault root path (`root`), run cycle-proof graph traversals (`query_symbolic`), save new episodic memories (`save`), record reinforcement feedback (`feedback`), or log a TiM abstract thought node (`thought`). |
| `manage_htr` | **Action**: `init` \| `ideate` \| `execute` \| `backprop` \| `merge` \| `run`<br>**Params**: `scope`, `hypothesis?`, `node_id?`, `files?`, `test_command?`, `max_steps?`, `status?`, `result?`, `insight?` | Initialize, ideate, execute, backpropagate, merge, or run a Cognitive Hypothesis Tree Search (HTR) workflow. |
| `manage_stm` | **Action**: `put` \| `get` \| `clear` \| `handoff`<br>**Params**: `session_id`, `key?`, `value?`, `parent_id?`, `subagent_id?`, `summary?`, `handoff_path?`, `scope?` | Set a session variable (`put`), read a variable (`get`), clear session storage (`clear`), or save an agent-to-subagent handoff (`handoff`). Uses strict transaction safety. |
| `manage_vault` | **Action**: `verify` \| `organize` \| `reprocess` \| `summarize` \| `audit` \| `ingest_bulk` \| `ingest_forge`<br>**Params**: `fix?`, `scope?`, `workspace_path?`, `source?`, `harness?` | Run DB-to-filesystem self-healing (`verify`), organize/deduplicate vault files (`organize`), reprocess missing embeddings (`reprocess`), summarize/dream over memories (`summarize`), run compliance audits (`audit`), bulk ingest logs (`ingest_bulk`), or chunk/forge PDFs (`ingest_forge`). |
| `manage_config` | **Action**: `get` \| `set`<br>**Params**: `provider?`, `duration?`, `model?`, `cloud_provider?`, `api_key?` | Retrieve (`get`) or update (`set`) LLM provider/model configurations. |
| `manage_file` | **Action**: `view` \| `replace` \| `multi_replace`<br>**Params**: `path`, `start_line?`, `end_line?`, `target_content?`, `replacement_content?`, `chunks?`, `allow_multiple?`, `instruction?`, `description?`, `is_skill_file?` | View files with virtual in-memory paging (`view`), surgically edit a contiguous block with placeholder resolution (`replace`), or patch multiple non-contiguous blocks (`multi_replace`). |
| `pre_invocation_hook` | **Params**: `session_id`, `query?`, `workspace_path?` | Execute the automatic hook to inject active POMDP belief states, stashed variables, and three-tier hybrid hydrated memories. |
| `complete_code_task` | **Params**: `task`, `files?`, `test_command?` | Execute reasoning and coding tasks in-process. |

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
3. **Enforced Memory Query**: If the pre-invocation hook output indicates no high-confidence memory episodes were found, or the query context is empty, you **MUST** manually invoke `manage_memory(action="search", query="...")` with a specific search query related to your task *before* writing any code.
4. **Log Episodic Memory**: After completing a task, call `manage_memory(action="save", title="...", content="...")` to persist your decisions and solutions.
5. **Record Feedback**: After running tests (e.g. `cargo test`) and verifying they pass, call `manage_memory(action="feedback", episode_id="...", success=true)` to reinforce retrieved wisdom.

---

## Agent Handoff Protocol & Smart Handoffs

When delegating work to a subagent, minimize context window usage and establish graph-linked memory associations:

### Spawning a Subagent

1. **Discover Vault Root**: Call `manage_memory(action="root")` to obtain the Obsidian vault root path (e.g., `/Users/keith/mythrax-vault`).
2. **Write the contract** to `<vault_root>/.handoffs/handoff_<task_id>.md`.
3. **Link context and register handoff**:
   - Write active context node record IDs to STM under key `"distilled_context_nodes"` using `manage_stm(action="put", session_id="...", key="distilled_context_nodes", value="...")`.
   - Call `manage_stm(action="handoff", ...)` to register the handoff, which automatically links it to the context nodes via `relates_to` edges in SurrealDB.
4. **Spawn the subagent** with a minimal prompt pointing to the vault handoff file:
   > *"Read and execute the handoff at `file:///<vault_root>/.handoffs/handoff_<task_id>.md` and rules at `file:///Users/keith/.gemini/AGENT.md`. Output first: `Execution Check: [Karpathy Rules applied? Yes/No] [Local Model verified? Yes/No/Fallback]`"*
5. **Hydrate context nodes**: The subagent reads the node IDs from STM (via `manage_stm(action="get")`) and calls `manage_memory(action="nodes", node_ids=[...])` to hydrate all active context nodes in a single call.
6. **Lazy context via file URLs**: Reference large files as links (`[file.rs](file:///path#L50-L100)`) — never paste content.
7. **AST Symbol & Code Structure Targeting**:
   - Use line-scoped `view_file` to read only the target AST symbols, preventing context window pollution from unrelated code.

---

## MemoryOS Paging & Paging-Aware Editing (v2.0)

To stay within LLM context windows and prevent token budget exhaustion while working with large codebases, Mythrax implements **MemoryOS Virtual Paging**:

### 1. Virtual In-Memory Skeletons (`view_file`)
*   When you read a source code file (like `.rs`, `.py`, `.js`, `.ts`) using `view_file`, the daemon automatically parses and pages out large symbol bodies (functions, structs, impls) to the `symbol_archive` SurrealDB table.
*   The tool returns a **virtual skeleton** containing lightweight placeholders (e.g. `[Paged Symbol: Reference page_fn_my_func]`) rather than the massive full-text bodies.
*   **Disk Integrity**: The physical file on disk is **never modified** during reads, keeping the codebase fully compiling.

### 2. Paging-Aware Editing (`replace_file_content`)
*   Because you only see the virtual skeleton, your target edit blocks will naturally contain placeholders.
*   The editing tools (`replace_file_content` and `multi_replace_file_content`) are **paging-aware**: they scan your target block for placeholders, query `symbol_archive` to fetch the original bodies, reconstruct the unpaged target block in memory, find and replace it in the physical disk file, and save it.
*   **How to Edit**: Simply target the placeholders exactly as they appear in the skeleton. The daemon handles the reconstruction surgically.

### 3. LRU Eviction & Context Pinning
*   The daemon's `PagingManager` maintains an active context queue of memory nodes in RAM.
*   When capacity is reached, it automatically evicts the Least Recently Used (LRU) memories.
*   **Context Pinning**: Wisdom rules, high-importance items ($\ge 8.0$), active handoffs, and active STM are pinned in RAM and are **immune** to LRU eviction.
