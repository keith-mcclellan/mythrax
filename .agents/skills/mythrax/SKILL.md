---
name: mythrax
description: Always query Mythrax memory via the MCP server before starting tasks or making edits, verify vault/knowledge graph integrity, and run HTR cognitive execution loops.
---

# Mythrax Unified Memory, Integrity & Cognitive Guidance

You are equipped with the **Mythrax** MCP server, which exposes tools for semantic memory storage, retrieval, reinforcement, compliance verification, vault integrity self-healing, and cognitive hypothesis execution.

## MCP Tools Reference
Use these native tools directly instead of executing custom scripts in the shell:
- `search_memories(query: str, scope: Optional[str] = "general", limit: int = 5)`: Execute a semantic vector search query over saved episodes.
- `search_wisdom(query: str, tier: str, limit: int = 5)`: Search wisdom rules.
- `save_episode(title: str, content: str, entities: List[dict], scope: Optional[str] = "general", vault_path: Optional[str] = None)`: Save a new episodic context.
- `record_feedback(id: str, success: bool)`: Apply reinforcement learning utility adjustment.
- `get_llm_config()`: Fetch the active LLM provider configurations.
- `update_llm_config(provider: str, duration: Optional[str] = "permanent", model: Optional[str] = None, cloud_provider: Optional[str] = None, api_key: Optional[str] = None)`: Update active model settings and API keys.
- `verify_compliance(workspace_path: Optional[str] = None)`: Execute all workspace compliance audits securely inside the MCP server.
- `bulk_ingest(source: str, harness: str, scope: Optional[str] = "general")`: Bulk ingest transcript logs from client harnesses.
- `organize_vault()`: Organize vault directories, deduplicate files, and maintain structure.
- `summarize_episodes(scope: Optional[str] = "general")`: Run compaction and dreaming cycles to summarize episodes into wisdom.
- `verify_vault_integrity(fix: bool = False)`: Execute database-to-filesystem and graph relationship verification, running self-healing repairs when `fix=true`.
- `reprocess_embeddings()`: Compute and save vector embeddings for episodes that were saved with missing model files.

## Compliance Requirements
1. **Always Search Memories First**: In every prompt turn, you must query memories at least once by invoking `search_memories`.
2. **Log Episodic Memory**: At the end of a coding task, save a summary of what you did using `save_episode` (automatically reinforced by git post-commit hooks).
3. **Record Feedback**: After running pytest or cargo test validation, call `record_feedback` to reinforce the utility of the retrieved wisdom.
4. **Self-Healing Integrity Audits**: Before dreaming or summarization, call `verify_vault_integrity(fix=true)` to align files on disk with the database cache.

## Cognitive Hypothesis Tree Search (HTR)
When executing HTR cognitive runs:
- Hypothesis nodes are stored in `wiki/<scope>/hypothesis_tree/<node_id>.md`.
- Ensure all test execution outputs and LLM critic reviews conform to the tree structure.
