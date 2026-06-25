# Mythrax Unified Memory, Ingestion & Cognitive Architecture User Guide

This guide serves as the definitive reference manual for **Project Mythrax**, a self-improving memory engine designed to extend agentic AI workflows with persistent episodic memory, semantic graph relationships, structured wisdom extraction, and zero-eager-prompting handoff protocols.

---

## 1. System Overview & Architecture

Mythrax 1.0 implements a client-server architecture to optimize resource utilization and ensure database integrity:

1. **Obsidian Vault (Filesystem Source of Truth)**: All episodes, insights, and wisdom rules are stored as clean markdown files under standardized folders. This allows users to read, edit, and navigate the entire memory graph using standard markdown links and tools like Obsidian.
2. **SurrealDB (Vector & Graph Query Engine)**: The database layer caches vault files, stores vector embeddings (using a centralized `nomic-embed-text-v1.5` ONNX model), and maintains directed relationship edges (`relates_to` and `parent_of`) for graph traversal and semantic search.
3. **Lightweight Thin Clients (MCP & CLI)**: The command line interface and the Model Context Protocol (MCP) server run as thin HTTP clients forwarding all requests to a persistent background daemon.
4. **Daemon Centrization**: The background daemon exclusively holds the RocksDB file write lock and the ONNX embedding model. This design resolves RocksDB lock contention, lowers individual client memory footprints by 50%, and makes query execution instantaneous.

```
                  ┌───────────────────────────────┐
                  │       Agent / IDE UI          │
                  └───────────────┬───────────────┘
                                  │ (MCP / CLI HTTP Requests)
                                  ▼
                  ┌───────────────────────────────┐
                  │   Mythrax Daemon (Port 8090)  │
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

### 1.1 Zero-CLI MCP Autonomy
Users who interact with Mythrax exclusively via the MCP server (e.g. through Cursor, VS Code, or other agent IDEs) do not need to run the CLI or manage daemon processes manually:
- **Automatic Daemon Boot**: On initialization of the MCP server, it automatically pings localhost port 8090. If the daemon is inactive, the MCP server automatically spawns the daemon in the background as a detached process.
- **Automatic Background Scheduling**: When the daemon boots, it automatically spawns the background scheduler tasks. This includes the Obsidian file watcher, the daily deep dreaming compaction, and the inactivity-debounced incremental synthesis loop.
- **Resource Cleanup**: When the host IDE shuts down and terminates the MCP server process, the daemon continues running in the background to serve subsequent client requests, maintaining its warm ONNX embedding cache and RocksDB single-writer integrity.

---

## 2. Core Memory Entities

The database and vault manage three main entities:

### 2.1 Episode
- **Description**: Raw historical transcripts, conversation logs, and actions.
- **Storage Location**: `vault/episodes/antigravity_<session_id>_part<N>_<hash>.md`
- **Token Optimization**: Long logs are chunked at a maximum of `100,000` characters to prevent prompt bloat and ONNX attention failures.

### 2.2 WikiNode (Artifact / Insight)
- **Description**: Distilled design documents, walkthroughs, chapter text, or synthesized insights.
- **Storage Location**: `vault/wiki/<scope>/insights/<name>.md` or `vault/wiki/artifacts/<session_id>/<name>.md`
- **Splicing**: Associated with episodes via directed `relates_to` graph edges.

### 2.3 WisdomRule
- **Description**: Empirical programming guidelines, habits, and anti-patterns extracted from successes and failures.
- **Storage Location**: `vault/wisdom/<tier>/<name>.md`
- **Schema**: Matches a structured format defining:
  - **Target Pattern**: What context triggers this rule.
  - **Action to Avoid**: Anti-patterns observed.
  - **Causal Explanation**: Why it is bad.
  - **Prescribed Remedy**: Surgical replacement or guideline.

---

## 3. Short Term Memory (STM) & Handoff Protocol

During multi-agent delegation, Mythrax enforces a **Zero-Eager-Prompting** model to share variable context and link memory nodes without prompt bloat.

### 3.1 Session Cache (STM)
- Short Term Memory stores temporary session key-value pairs in SurrealDB and dual-writes them to a local JSON file at `.handoffs/stm_<session_id>.json`.
- API keys and secrets are automatically masked via a `SecretFilter` before hitting the disk.
- Running the `clear` action on STM purges both the database record and the local JSON file.

### 3.2 Smart Handoff Flow
1. **Discover Vault**: The parent agent queries the vault root to locate the active vault path.
2. **Context Setup**: The parent writes target node record IDs (like `["wiki_node:design_spec", "wisdom:rust_rules"]`) to STM under the key `"distilled_context_nodes"`.
3. **Register Handoff**: The parent saves the handoff, specifying the handoff contract markdown file path. The backend automatically reads `"distilled_context_nodes"` and creates `relates_to` graph edges linking the handoff record to the target nodes.
4. **Subagent Spawn**: The parent spawns the subagent with a minimal prompt pointing to the handoff path.
5. **Context Hydration**: The subagent reads the node IDs from STM and retrieves them to hydrate all context in a single call, preventing full-file text injection.

---

## 4. Ingestion & Forging Pipeline

### 4.1 Ingestion (`mythrax ingest bulk`)
- Walks the target history folder, parses log transcripts, chunks them at `100,000` characters, and saves them.
- Detects and parses markdown attachments (artifacts), saving them as `WikiNode` records and establishing bidirectional graph links with their corresponding episode chunks.

### 4.2 Document Forging (`mythrax ingest forge`)
- Exposes an automated ingestion pipeline for PDFs and raw text manuals.
- Chunks files into 2,000-token windows with 10% overlap.
- Extracts structured `WisdomRule`s and `WikiNode`s, writing them to `vault/wisdom/forge/` and `vault/wiki/forge/`.
- **Automated Skill Skeletonization**: Parses long `SKILL.md` files, extracts verbose examples to `examples/` and references to `references/` subdirectories, and rewrites `SKILL.md` as a lean playbook.

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

The `mythrax` CLI is located in the `mythrax-core` directory. Commands automatically route requests to the daemon over HTTP on port `8090` using the security token found in `~/.mythrax/token`. If the daemon is inactive, the CLI automatically spawns the background daemon and polls for 5 seconds before executing the command.

| Group / Subcommand | Arguments | Purpose |
|--------------------|-----------|---------|
| `mythrax init` | `[harness]` | Bootstraps configuration folders and auto-ingests transcripts. |
| `mythrax daemon start` | `--port <num>` | Launches the REST API daemon on the specified port in the background. |
| `mythrax daemon stop` | None | Terminates the background daemon using its tracked PID. |
| `mythrax daemon run` | `--port <num>` | Runs the daemon in the foreground (useful for local testing and logs). |
| `mythrax memory query` | `<query> [flags]` | Queries long-term memory using vector search (or substring fallback). |
| `mythrax memory record`| `--file <path>` | Saves a markdown file as an episodic memory in the vault. |
| `mythrax memory feedback`| `<id> <success>`| Records success/failure feedback for a rule. |
| `mythrax memory root` | None | Retrieves the active Obsidian vault root directory path. |
| `mythrax htr <action>` | `[args]` | Manages Hypothesis-Tree Refinement (Arbor) loops (`init`, `ideate`, `execute`, `backprop`, `merge`, `run`). |
| `mythrax stm <action>` | `[args]` | Manages short-term memory keys and handoffs (`put`, `get`, `clear`, `handoff`). |
| `mythrax vault <action>`| `[args]` | Manages vault lifecycle operations (`organize`, `verify`, `reprocess`, `summarize`). |
| `mythrax config <action>`| `[args]` | Gets or sets LLM provider configurations. |
| `mythrax ingest bulk` | `--source <path>` | Idempotently ingests log transcripts and files. |
| `mythrax ingest forge`| `<source_path>` | Chunks and parses manuals or documents into rules/wiki nodes. |
| `mythrax audit` | `[workspace]` | Runs safety compliance audits on the active directory. |

---

## 8. MCP Tools Reference

The MCP server exposes 9 high-efficiency, action-enum-based tools to client harnesses, cutting schema token overhead by over 60% compared to legacy granular tools.

### 8.1 `query_memory`
- **Description**: Query the memory graph for episodes, rules, nodes, or root context.
- **Actions**:
  - `search`: Performs semantic vector search (with text substring fallback if embeddings are missing).
  - `rules`: Retrieves wisdom rules matching a target pattern.
  - `nodes`: Retrieves specific database nodes by their IDs.
  - `root`: Returns the active Obsidian vault root path.

### 8.2 `record_memory`
- **Description**: Save new episodes or record feedback on existing ones.
- **Actions**:
  - `save`: Saves an episodic memory to the vault and indexes it in the database.
  - `feedback`: Registers success or failure feedback for a wisdom rule.

### 8.3 `manage_htr`
- **Description**: Manages Hypothesis-Tree Refinement (Arbor) loops.
- **Actions**:
  - `init`, `ideate`, `execute`, `backprop`, `merge`, `run`.

### 8.4 `manage_stm`
- **Description**: Manages short-term session variables and handoff contracts.
- **Actions**:
  - `put`, `get`, `clear`, `handoff`.

### 8.5 `manage_vault`
- **Description**: Manages vault database maintenance and lifecycle.
- **Actions**:
  - `verify`, `organize`, `reprocess`, `summarize`.

### 8.6 `manage_config`
- **Description**: Retrieves or updates active LLM configurations.
- **Actions**:
  - `get`, `set`.

### 8.7 `compliance_audit`
- **Description**: Runs safety compliance audits on the active directory (checks Tailwind blocks, search history).

### 8.8 `ingest_knowledge`
- **Description**: Ingests transcript logs or documents.
- **Actions**:
  - `bulk`: Ingests conversation transcripts in bulk.
  - `forge`: Extracts structured rules and wiki nodes from text or PDF source documents.
  - `save_forged_assets`: Saves batches of forged sections.

### 8.9 `pre_invocation_hook`
- **Description**: Executes automatic pre-invocation compliance routines.

---

## 9. Troubleshooting & Lock Resolution

### 9.1 Single-Writer Architecture
RocksDB requires an exclusive file lock on its database directory. Under the 1.0 architecture, lock contention is avoided because **only the daemon process interacts with the database files**.
- All CLI commands and MCP server instances communicate with the daemon via HTTP on port `8090`.
- Multiple clients can run concurrently without encountering "Resource temporarily unavailable" lock errors.

### 9.2 Stale Processes & Force Unlocking
If the daemon becomes unresponsive or port `8090` is blocked by a zombie process:
1. Stop all processes:
   ```bash
   ./scripts/maintain_mythrax.sh stop
   ```
2. Clean up stale PID and lock files:
   ```bash
   ./scripts/maintain_mythrax.sh unlock
   ```
3. Restart the daemon:
   ```bash
   ./scripts/maintain_mythrax.sh start
   ```
