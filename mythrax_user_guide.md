# Mythrax Unified Memory, Ingestion & Cognitive Architecture User Guide

This guide serves as the definitive reference manual for **Project Mythrax**, a self-improving memory engine designed to extend agentic AI workflows with persistent episodic memory, semantic graph relationships, structured wisdom extraction, and zero-eager-prompting handoff protocols.

---

## 1. System Overview & Architecture

Mythrax implements a dual-layer storage model that bridges a human-readable filesystem and a high-performance vector/graph database:

1. **Obsidian Vault (Filesystem Source of Truth)**: All episodes, insights, and wisdom rules are stored as clean markdown files under standardized folders. This allows developers to read, edit, and navigate the entire memory graph using standard markdown links and tools like Obsidian.
2. **SurrealDB (Vector & Graph Query Engine)**: Cache layer that indexes the markdown files, stores vector embeddings (using a local `nomic-embed-text-v1.5` ONNX model), and maintains directed relationship edges (`relates_to` and `parent_of`) for rapid graph traversal and semantic search.

```
                  ┌───────────────────────────────┐
                  │       Agent / IDE UI          │
                  └───────────────┬───────────────┘
                                  │ (MCP / CLI)
                                  ▼
                  ┌───────────────────────────────┐
                  │      Mythrax Core (Rust)      │
                  └──────┬─────────────────┬──────┘
                         │                 │
            (Filesystem) │                 │ (RocksDB / Mem)
                         ▼                 ▼
                  ┌─────────────┐   ┌─────────────┐
                  │  Obsidian   │   │  SurrealDB  │
                  │  Markdown   │   │   (Graph/   │
                  │    Vault    │   │   Vector)   │
                  └─────────────┘   └─────────────┘
```

---

## 2. Core Memory Entities

The database and vault manage three main entities:

### 2.1 Episode
* **Description**: Raw historical transcripts, conversation logs, and actions.
* **Storage Location**: `vault/episodes/antigravity_<session_id>_part<N>_<hash>.md`
* **Token Optimization**: Long logs are chunked at a maximum of `100,000` characters to prevent prompt bloat and ONNX attention failures.

### 2.2 WikiNode (Artifact / Insight)
* **Description**: Distilled design documents, code walkthroughs, chapter text, or synthesized insights.
* **Storage Location**: `vault/wiki/<scope>/insights/<name>.md` or `vault/wiki/artifacts/<session_id>/<name>.md`
* **Splicing**: Associated with episodes via directed `relates_to` graph edges.

### 2.3 WisdomRule
* **Description**: Empirical programming guidelines, habits, and anti-patterns extracted from successes and failures.
* **Storage Location**: `vault/wisdom/<tier>/<name>.md`
* **Schema**: Matches a structured format defining:
  * **Target Pattern**: What context triggers this rule.
  * **Action to Avoid**: Anti-patterns observed.
  * **Causal Explanation**: Why it's bad.
  * **Prescribed Remedy**: Surgical replacement or guideline.

---

## 3. Short Term Memory (STM) & Handoff Protocol

During multi-agent delegation, Mythrax enforces a **Zero-Eager-Prompting** model to share variable context and link memory nodes without prompt bloat.

### 3.1 Session Cache (STM)
* Short Term Memory stores temporary session key-value pairs in SurrealDB and dual-writes them to a local JSON file at `.handoffs/stm_<session_id>.json`.
* API keys and secrets are automatically masked via a `SecretFilter` before hitting the disk.
* Calling `clear_short_term` purges both the database record and the local JSON file.

### 3.2 Smart Handoff Flow
1. **Discover Vault**: The parent agent calls `get_vault_root` to locate the active vault.
2. **Context Setup**: The parent writes target node record IDs (like `["wiki_node:design_spec", "wisdom:rust_rules"]`) to STM under the key `"distilled_context_nodes"`.
3. **Register Handoff**: The parent calls `save_handoff`, specifying the handoff contract markdown file path. The backend automatically reads `"distilled_context_nodes"` and creates `relates_to` graph edges linking the handoff record to the target nodes.
4. **Subagent Spawn**: The parent spawns the subagent with a minimal prompt:
   > *"Read and execute the handoff contract at `file:///.handoffs/handoff_<id>.md`. Run `get_short_term` to retrieve distilled node IDs and hydrate them."*
5. **Context Hydration**: The subagent reads the node IDs from STM and calls `get_memory_nodes` to hydrate all context in a single call, preventing full-file text injection.

---

## 4. Ingestion & Forging Pipeline

### 4.1 Ingestion (`vault ingest`)
* Walks the target history folder, parses log transcripts, chunks them at `100,000` characters, and saves them.
* Detects and parses markdown attachments (artifacts), saving them as `WikiNode` records and establishing bidirectional graph links with their corresponding episode chunks.

### 4.2 Document Forging (`forge_source`)
* Exposes an automated ingestion pipeline for PDFs and raw text manuals.
* Chunks files into 2,000-token windows with 10% overlap.
* Uses the local LLM to extract structured `WisdomRule`s and `WikiNode`s, writing them to `vault/wisdom/forge/` and `vault/wiki/forge/`.
* **Automated Skill Skeletonization**: Parses long `SKILL.md` files, extracts verbose examples to `examples/` and references to `references/` subdirectories, and rewrites `SKILL.md` as a lean (<200 token) rule playbook.

---

## 5. Cognitive Synthesis (Dreaming & Compaction)

The system automatically runs dreaming cycles (either incremental or daily) to prevent long-term memory degradation:
1. **DBSCAN Clustering**: Clusters unprocessed episodes using cosine similarity on their vector embeddings.
2. **LLM Synthesis**: For each cluster, the engine queries the database, grabs the full raw content of all related `WikiNode` artifacts, and feeds the merged transcript and artifact context to the LLM.
3. **Compaction**: The LLM output is saved as a new high-level `WikiNode` (Insight) or a `WisdomRule`, and the source episodes are marked as `processed_in_dream = true`.

---

## 6. Precedence Rules & Conflict Resolution

When retrieved memories, rules, or user prompts conflict, apply this deterministic hierarchy (highest wins):

1. **User Prompt / Workspace Rules (`AGENT.md` / `AGENTS.md`)** — Absolute precedence.
2. **Active Workspace Skills** (`.agents/skills/<name>/SKILL.md`) — Overrides global.
3. **Developer / Empirical Episodes** — Dynamic logs and unit test feedback.
4. **Ingested Forge Wisdom** (`tier: "forge"`) — Static rules from manuals/books.
5. **Global / Default Skills** — Lowest precedence.

*Note: For conflicts within the same tier, the record with the most recent `updated_at` timestamp wins.*

---

## 7. CLI Command Reference

The `mythrax` CLI is located in `mythrax-core` and can be run via `cargo run --bin mythrax -- [command]`.

| Command | Arguments | Purpose |
|---------|-----------|---------|
| `init` | `<harness>` | Bootstraps configuration folders and auto-ingests transcripts. |
| `status` | None | Checks the status of the configuration files. |
| `vault ingest` | `--source <path> --harness <type> --scope <name>` | Idempotently ingests transcripts. |
| `vault reprocess`| None | Computes embeddings for database records missing vectors. |
| `vault verify` | `--fix` | Scans the database against the markdown files and heals mismatches. |
| `vault summarize`| None | Runs DBSCAN clustering and LLM compactions. |
| `daemon start` | `--port <num>` | Launches the REST API daemon on the specified port. |
| `daemon stop` | None | Terminates the background daemon. |

---

## 8. MCP Tools Reference

When interacting as an MCP server, the following tools are exposed to client harnesses:

* **`search_memories(query, scope?, limit?, token_budget?, include_episodes?)`**: Semantic vector search. By default, ignores raw artifacts and logs to save context window.
* **`search_wisdom(query, tier, limit?)`**: Retrieves wisdom rules matching a pattern.
* **`save_episode(title, content, entities, scope?, vault_path?)`**: Saves a new episode.
* **`put_short_term(session_id, key, value)`**: Writes to short term memory.
* **`get_short_term(session_id, key)`**: Reads from short term memory.
* **`clear_short_term(session_id)`**: Clears the session variables and disk files.
* **`save_handoff(parent_conversation_id, subagent_conversation_id, summary, handoff_file_path, scope?)`**: Registers a task contract and creates database relationships.
* **`get_memory_nodes(node_ids)`**: Hydrates multiple records (Episodes/WikiNodes) by ID.
* **`forge_source(source_path, scope?)`**: Chunks and parses reference documents.
* **`verify_compliance(workspace_path?)`**: Runs static linter checks (Tailwind block, search history).

---

## 9. Troubleshooting & Lock Resolution

### 9.1 RocksDB Lock Conflict
Because RocksDB is a single-process engine, starting a daemon while the IDE has an active `mythrax mcp` server running will throw a lock acquisition failure. 
* **To resolve**: Keep the background daemon stopped/unloaded while working in the IDE.
* **To force kill all instances**: `pkill -f mythrax`

### 9.2 LaunchAgent Control
The background daemon is managed via `launchd` on macOS:
```bash
# Stop/Unload
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.mythrax.daemon.plist

# Start/Load
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.mythrax.daemon.plist

# Restart running process
launchctl kickstart -k gui/$(id -u)/com.mythrax.daemon
```
