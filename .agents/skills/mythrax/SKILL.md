---
name: mythrax
description: Always query Mythrax memory via the MCP server before starting tasks or making edits, verify vault/knowledge graph integrity, and run HTR cognitive execution loops.
---

# Mythrax Unified Memory, Integrity & Cognitive Guidance

You are equipped with the **Mythrax** MCP server, which exposes tools for semantic memory storage, retrieval, reinforcement, compliance verification, vault integrity self-healing, cognitive hypothesis execution, short-term memory sharing between agents, and document ingestion via Forge.

---

## MCP Tools Reference

Use these native tools directly instead of executing custom scripts in the shell:

| Tool | Signature | Purpose |
|------|-----------|---------|
| `search_memories` | `(query, scope?, limit?, token_budget?, allow_downward?)` | Semantic vector search over saved episodes |
| `search_wisdom` | `(query, tier, limit?)` | Search wisdom rules by tier |
| `save_episode` | `(title, content, entities, scope?, vault_path?)` | Persist episodic context |
| `record_feedback` | `(id, success)` | Reinforcement learning utility adjustment |
| `get_llm_config` | `()` | Fetch active LLM provider configuration |
| `update_llm_config` | `(provider, duration?, model?, cloud_provider?, api_key?)` | Update model settings |
| `verify_compliance` | `(workspace_path?)` | Execute workspace compliance audits |
| `bulk_ingest` | `(source, harness, scope?)` | Bulk ingest transcript logs |
| `organize_vault` | `()` | Organize vault directories, deduplicate |
| `summarize_episodes` | `(scope?)` | Run compaction + dreaming cycles |
| `verify_vault_integrity` | `(fix?)` | DB-to-filesystem self-healing verification |
| `reprocess_embeddings` | `()` | Compute embeddings for episodes missing vectors |
| `put_short_term` | `(session_id, key, value)` | Write a key-value pair to STM for the session |
| `get_short_term` | `(session_id, key)` | Read a key-value pair from STM |
| `clear_short_term` | `(session_id)` | Clear STM and delete `.handoffs/stm_<session_id>.json` |
| `save_handoff` | `(parent_conversation_id, subagent_conversation_id, summary, handoff_file_path, scope?)` | Save parent-to-subagent task handoff and link context nodes |
| `get_memory_nodes` | `(node_ids)` | Hydrate specific database records by record IDs |
| `forge_source` | `(source_path, scope?)` | Ingest a document (PDF/text/markdown) to extract WisdomRules and WikiNodes |

---

## Compliance Requirements

1. **Always Search Memories First**: In every prompt turn, query memories at least once via `search_memories`.
2. **Log Episodic Memory**: After a coding task, call `save_episode` with a summary of decisions.
3. **Record Feedback**: After `cargo test` or `pytest` passes, call `record_feedback` to reinforce retrieved wisdom.
4. **Self-Healing Integrity Audits**: Before dreaming or summarization, call `verify_vault_integrity(fix=true)`.

---

## Agent Handoff Protocol (Zero-Eager-Prompting) & Smart Handoffs (v0.3.0)

When delegating work to a subagent, minimize context window usage and establish graph-linked memory associations:

### Spawning a Subagent

1. **Write the contract** to `.handoffs/handoff_<task_id>.md` at the workspace root.
2. **Link context and register handoff**:
   - Write active context node record IDs (e.g. WikiNodes or WisdomRules) to STM under key `"distilled_context_nodes"` using the `put_short_term` tool.
   - Call the `save_handoff` MCP tool to create the handoff record and automatically link it to the context nodes via `relates_to` edges in SurrealDB.
3. **Spawn the subagent** with a minimal prompt:
   > *"Read and execute the handoff at `file:///absolute/path/.handoffs/handoff_<task_id>.md` and rules at `file:///Users/keith/.gemini/AGENT.md`. Output first: `Execution Check: [Karpathy Rules applied? Yes/No]`"*
4. **Hydrate context nodes**: The subagent reads the node IDs from STM (via `get_short_term`) and calls the `get_memory_nodes` MCP tool to hydrate the active context nodes in a single call.
5. **Lazy context via file URLs**: Reference large files as links (`[file.rs](file:///path#L50-L100)`) — never paste content.

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

### LLM Sub-Call Optimization

When calling `openai_chat` or similar stateless LLM APIs:
- **Scoped prompts**: Pass only the target function/class signature, not whole files.
- **Diff-only output**: Instruct the model to return search-and-replace blocks only.
- **Strip non-functional tokens**: Remove docstrings, dead imports before injection.
- **No-explanation constraint**: System prompt must say "return only code, zero explanation".
- **Bound max_tokens**: Set to expected output size to prevent runaway generation.

---

## Short Term Memory (STM) & Smart Handoffs (v0.3.0)

STM is a lightweight key-value store shared between parent and subagent during a session. It persists to SurrealDB, is dual-written to `.handoffs/stm_<session_id>.json`, and is used to dynamically link context nodes during handoffs.

### 1. Basic Key-Value Sharing
```python
# Parent writes active variables before spawning subagent
put_short_term(session_id="abc123", key="target_file", value="/path/to/file.rs")
put_short_term(session_id="abc123", key="error_context", value="SurrealDB id field mismatch")

# Subagent reads them
get_short_term(session_id="abc123", key="target_file")
```

### 2. Smart Handoffs (Graph-Linked Handoffs)
During handoff, the parent can store a list of active node record IDs in STM under the key `"distilled_context_nodes"`. When saving the handoff via the `save_handoff` tool, the backend automatically reads this key, creates the handoff record, and links it to those nodes via `relates_to` edges. The subagent can then retrieve the IDs and hydrate all nodes in a single call.

```python
# 1. Parent writes active node IDs to its STM
put_short_term(
    session_id="parent_session_id",
    key="distilled_context_nodes",
    value='["wiki_node:insights_123", "wisdom:rule_456"]'
)

# 2. Parent saves the handoff via MCP (automatically links the handoff to the context nodes via relates_to edges)
save_handoff(
    parent_conversation_id="parent_session_id",
    subagent_conversation_id="subagent_session_id",
    summary="Task brief summary",
    handoff_file_path=".handoffs/handoff_task_123.md",
    scope="general"
)

# 3. Subagent reads key from its STM
get_short_term(
    session_id="subagent_session_id",
    key="distilled_context_nodes"
)

# 4. Subagent hydrates all context nodes in one call
get_memory_nodes(
    node_ids=["wiki_node:insights_123", "wisdom:rule_456"]
)
```

# Parent clears after task completes (also deletes the local .json file)
clear_short_term(session_id="parent_session_id")

**Security**: STM values are sanitized by `SecretFilter` before writing to disk (API keys and tokens are masked).

---

## Forge: Document Ingestion Pipeline

Use `forge_source` to extract structured knowledge from reference documents (PDFs, books, skill guides):

```
forge_source(source_path="/path/to/guide.pdf", scope="coding")
```

Forge will:
1. Extract text (PDF via `pdf-extract`, or raw text for `.md`/`.txt`)
2. Chunk into 2000-token windows with 10% overlap
3. Call the local LLM to extract `WisdomRule`s and `WikiNode`s per chunk
4. Write markdown files to `vault/wisdom/forge/` and `vault/wiki/forge/`
5. Persist to SurrealDB for semantic search

### Automated Skill Skeletonization

Verbose `SKILL.md` files should be lean (< 200 tokens of rules). Run Forge to skeletonize:
- `## Examples` sections → extracted to `examples/examples.md`
- `## References` sections → extracted to `references/references.md`
- `SKILL.md` is rewritten with pointers to subdirectory files

---

## Memory Conflict Resolution

When retrieved memories conflict, apply this precedence hierarchy (highest wins):

1. **User Prompt / `AGENT.md` / `AGENTS.md`** — absolute precedence
2. **Active workspace skills** (`.agents/skills/<name>/SKILL.md`) — overrides global
3. **Developer/empirical episodes** — dynamic debugging memories override static docs
4. **Ingested Forge wisdom** (`tier: "forge"`) — static guidelines from books/manuals
5. **Global default skills** — lowest precedence

Within the same tier, the **more recent** record (higher `updated_at`) wins.

**If a conflict is found**: Document it in the implementation plan under "User Review Required" — showing the conflict, the rule applied, and the resolution. If completely unresolvable (equal rank + age), prompt the user directly.

---

## Pagination-Aware Search

Search results include a `PAGINATION NOTICE` when more results exist. Follow-up fetching rules:
- **Skills / wisdom matches**: Follow-up pagination is **required**.
- **Developer episodes / logs**: Follow-up pagination is **optional** (recommended for complex tasks).

---

## Cognitive Hypothesis Tree Search (HTR)

When executing HTR cognitive runs:
- Hypothesis nodes live at `wiki/<scope>/hypothesis_tree/<node_id>.md`
- Use `htr_run` for automated end-to-end loops, or `htr_init` → `htr_ideate` → `htr_execute` → `htr_backprop` → `htr_merge` for manual control
- All test execution outputs and LLM critic reviews must conform to the tree structure
