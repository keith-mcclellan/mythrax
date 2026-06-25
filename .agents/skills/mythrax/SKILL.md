---
name: mythrax
description: Always query Mythrax memory via the MCP server before starting tasks or making edits, verify vault/knowledge graph integrity, and run HTR cognitive execution loops.
---

# Mythrax Unified Memory, Integrity & Cognitive Guidance

You are equipped with the **Mythrax** MCP server, which exposes tools for semantic memory storage, retrieval, reinforcement, compliance verification, vault integrity self-healing, cognitive hypothesis execution, short-term memory sharing between agents, and document ingestion via Forge.

---

## MCP Tools Reference

All legacy granular tools (32 tools) have been consolidated into 9 high-efficiency, action-enum-based tools to reduce context schema bloat and prevent token overhead. Use these consolidated tools directly:

| Tool | Action Enum / Parameters | Purpose |
|------|---------------------------|---------|
| `query_memory` | **Action**: `search` \| `rules` \| `nodes` \| `root` \| `query_symbolic`<br>**Params**: `query?`, `scope?`, `limit?`, `token_budget?`, `allow_downward?`, `node_ids?`, `node_id?`, `relation?`, `max_depth?` | Search memories (`search`), find wisdom rules (`rules`), hydrate node IDs (`nodes`), get the vault root path (`root`), or run cycle-proof graph traversals (`query_symbolic`). |
| `record_memory` | **Action**: `save` \| `feedback` \| `thought`<br>**Params**: `title?`, `content?`, `entities?`, `scope?`, `vault_path?`, `episode_id?`, `success?` | Save a new episodic memory (`save`), record reinforcement feedback (`feedback`), or log a TiM abstract thought node (`thought`) to `wiki/thoughts/`. |
| `manage_file` | **Action**: `view` \| `replace` \| `multi_replace`<br>**Params**: `path`, `start_line?`, `end_line?`, `target_content?`, `replacement_content?`, `chunks?`, `allow_multiple?`, `instruction?`, `description?`, `is_skill_file?` | View files with virtual in-memory paging (`view`), surgically edit a contiguous block with placeholder resolution (`replace`), or patch multiple non-contiguous blocks (`multi_replace`). |
| `manage_htr` | **Action**: `init` \| `ideate` \| `execute` \| `backprop` \| `merge` \| `run`<br>**Params**: `scope`, `hypothesis?`, `node_id?`, `files?`, `test_command?`, `max_steps?`, `status?`, `result?`, `insight?` | Initialize, ideate, execute, backpropagate, merge, or run a Cognitive Hypothesis Tree Search (HTR) workflow. |
| `manage_stm` | **Action**: `put` \| `get` \| `clear` \| `handoff`<br>**Params**: `session_id`, `key?`, `value?`, `parent_id?`, `subagent_id?`, `summary?`, `handoff_path?`, `scope?` | Set a session variable (`put`), read a variable (`get`), clear session storage (`clear`), or save an agent-to-subagent handoff (`handoff`). Uses strict transaction safety. |
| `manage_vault` | **Action**: `verify` \| `organize` \| `reprocess` \| `summarize` \| `audit`<br>**Params**: `fix?`, `scope?`, `workspace_path?` | Run DB-to-filesystem self-healing (`verify`), organize/deduplicate vault files (`organize`), reprocess missing embeddings (`reprocess`), summarize/dream over episodes (`summarize`), or run compliance audits (`audit`). |
| `manage_config` | **Action**: `get` \| `set`<br>**Params**: `provider?`, `duration?`, `model?`, `cloud_provider?`, `api_key?` | Retrieve (`get`) or update (`set`) LLM provider/model configurations. |
| `ingest_knowledge`| **Action**: `bulk` \| `forge`<br>**Params**: `source`, `harness?`, `scope?` | Bulk ingest transcript logs (`bulk`) or parse and extract WisdomRules/WikiNodes from PDFs/documents via Forge (`forge`). |
| `pre_invocation_hook` | **Params**: `session_id`, `query?`, `workspace_path?` | Execute the automatic hook to inject active POMDP belief states, stashed variables, and three-tier hybrid hydrated memories. |

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
3. **Enforced Memory Query**: If the pre-invocation hook output indicates no high-confidence memory episodes were found, or the query context is empty, you **MUST** manually invoke `query_memory(action="search", query="...")` with a specific search query related to your task *before* writing any code.
4. **Log Episodic Memory**: After completing a task, call `record_memory(action="save", title="...", content="...")` to persist your decisions and solutions.
5. **Record Feedback**: After running tests (e.g. `cargo test`) and verifying they pass, call `record_memory(action="feedback", episode_id="...", success=true)` to reinforce retrieved wisdom.

---

## Agent Handoff Protocol (Zero-Eager-Prompting) & Smart Handoffs

When delegating work to a subagent, minimize context window usage and establish graph-linked memory associations:

### Spawning a Subagent

1. **Discover Vault Root**: Call `query_memory(action="root")` to obtain the Obsidian vault root path (e.g., `/Users/keith/mythrax-vault`).
2. **Write the contract** to `<vault_root>/.handoffs/handoff_<task_id>.md`.
3. **Link context and register handoff**:
   - Write active context node record IDs to STM under key `"distilled_context_nodes"` using `manage_stm(action="put", session_id="...", key="distilled_context_nodes", value="...")`.
   - Call `manage_stm(action="handoff", ...)` to register the handoff, which automatically links it to the context nodes via `relates_to` edges in SurrealDB.
4. **Spawn the subagent** with a minimal prompt pointing to the vault handoff file:
   > *"Read and execute the handoff at `file:///<vault_root>/.handoffs/handoff_<task_id>.md` and rules at `file:///Users/keith/.gemini/AGENT.md`. Output first: `Execution Check: [Karpathy Rules applied? Yes/No] [Local Model verified? Yes/No/Fallback]`"*
5. **Hydrate context nodes**: The subagent reads the node IDs from STM (via `manage_stm(action="get")`) and calls `query_memory(action="nodes", node_ids=[...])` to hydrate all active context nodes in a single call.
6. **Lazy context via file URLs**: Reference large files as links (`[file.rs](file:///path#L50-L100)`) — never paste content.
7. **AST Symbol & Code Structure Targeting**:
   - Use line-scoped `view_file` to read only the target AST symbols, preventing context window pollution from unrelated code.

### Handoff Contract Template (`.handoffs/handoff_<task_id>.md`)

```markdown
# Agent Handoff: [Task Name]
- **From:** [Parent Agent ID]  **To:** [Subagent ID]  **Status:** PENDING
- **STM Session ID:** [Session ID]
- **Distilled Context Nodes:** [List of record IDs (e.g. `["wiki_node:insights_123", "wisdom:rule_456"]`)]

## 1. Objective & Scope
[What to build/fix. What is explicitly out of scope.]

## 2. Success Criteria
- [ ] Verifiable criterion 1
- [ ] Verifiable criterion 2

## 3. Scoped Target Context
- **Target:** [file.rs](file:///absolute/path/file.rs#L10-L50)
- **AST Symbol:** `impl Forge::ingest_document`

## 4. Verification Command
`cargo test --test test_forge`

## 5. Assumptions & Tradeoffs
- **Assumption:** [Document it]
```

### Return Handoff Template (`.handoffs/handoff_<task_id>_return.md`)

```markdown
# Agent Handoff Return: [Task Name]
- **Status:** COMPLETED / FAILED

## 1. Summary of Changes
## 2. Modified Files
## 3. Verification Results
## 4. Context Preservation (edge cases, remaining work)
```

---

## Short Term Memory (STM) & Smart Handoffs

STM is a lightweight key-value store shared between parent and subagent during a session. It persists to SurrealDB, is dual-written to `.handoffs/stm_<session_id>.json`, and is used to dynamically link context nodes during handoffs.

### 1. Basic Key-Value Sharing
```python
# Parent writes active variables before spawning subagent
manage_stm(action="put", session_id="abc123", key="target_file", value="/path/to/file.rs")
manage_stm(action="put", session_id="abc123", key="error_context", value="SurrealDB id field mismatch")

# Subagent reads them
manage_stm(action="get", session_id="abc123", key="target_file")
```

### 2. Smart Handoffs (Graph-Linked Handoffs)
During handoff, the parent can store a list of active node record IDs in STM under the key `"distilled_context_nodes"`. When saving the handoff via `manage_stm(action="handoff")`, the backend automatically reads this key, creates the handoff record, and links it to those nodes via `relates_to` edges.

```python
# 1. Parent writes active node IDs to its STM
manage_stm(
    action="put",
    session_id="parent_session_id",
    key="distilled_context_nodes",
    value='["wiki_node:insights_123", "wisdom:rule_456"]'
)

# 2. Parent saves the handoff via MCP (automatically links the handoff to the context nodes via relates_to edges)
manage_stm(
    action="handoff",
    session_id="parent_session_id",
    parent_id="parent_session_id",
    subagent_id="subagent_session_id",
    summary="Task brief summary",
    handoff_path=".handoffs/handoff_task_123.md",
    scope="general"
)
```

---

## Forge: Document Ingestion Pipeline

Use `ingest_knowledge(action="forge")` to extract structured knowledge from reference documents (PDFs, books, skill guides):

```python
ingest_knowledge(action="forge", source="/path/to/guide.pdf", scope="coding")
```

Forge will:
1. Extract text (PDF via `pdf-extract`, or raw text for `.md`/`.txt`).
2. **Granular Semantic Chunking (1,000–2,000 tokens)**: Chunk the document into smaller, highly-focused segments of 1,000 to 2,000 tokens using paragraph boundaries, optimizing vector search precision and local model processing speed.
3. **Structured Bidirectional Relations**: Link all chunks to the parent document node and establish bidirectional sequential relations between adjacent chunks (`Chunk N <-> Chunk N+1` of type `next`/`prev`). This allows high-precision surrounding context retrieval during search.
4. Call the local LLM to extract `WisdomRule`s and `WikiNode`s per chunk.
5. Write markdown files to `vault/wisdom/forge/` and `vault/wiki/forge/`.
6. Persist to SurrealDB for semantic search.

---

## Memory Conflict Resolution

When retrieved memories conflict, apply this precedence hierarchy (highest wins):

1. **User Prompt / `AGENT.md` / `AGENTS.md`** — absolute precedence
2. **Active workspace skills** (`.agents/skills/<name>/SKILL.md`) — overrides global
3. **Developer/empirical episodes** — dynamic debugging memories override static docs
4. **Ingested Forge wisdom** (`tier: "forge"`) — static guidelines from books/manuals
5. **Global default skills** — lowest precedence

Within the same tier, the **more recent** record (higher `updated_at`) wins.

---

## Cognitive Hypothesis Tree Search (HTR)

When executing HTR cognitive runs:
- Hypothesis nodes live at `wiki/<scope>/hypothesis_tree/<node_id>.md`
- Use `manage_htr(action="run")` for automated end-to-end loops, or `manage_htr` with specific actions (`init`, `ideate`, `execute`, `backprop`, `merge`) for manual control.
- All test execution outputs and LLM critic reviews must conform to the tree structure.

---

## MemoryOS Paging & Paging-Aware Editing (v1.2)

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
