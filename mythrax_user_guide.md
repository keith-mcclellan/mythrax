# Mythrax Unified Memory, Ingestion & Cognitive Architecture User Guide

This guide serves as the definitive reference manual for **Project Mythrax**, a self-improving memory engine designed to extend agentic AI workflows with persistent episodic memory, semantic graph relationships, structured wisdom extraction, and zero-eager-prompting handoff protocols.

---

## 1. System Overview & Architecture

Mythrax 2.3.2 implements a client-server architecture to optimize resource utilization and ensure database integrity:

1. **Obsidian Vault (Filesystem Source of Truth)**: All episodes, insights, and wisdom rules are stored as clean markdown files under standardized folders. This allows users to read, edit, and navigate the entire memory graph using standard markdown links and tools like Obsidian.
2. **SurrealDB (Vector & Graph Query Engine)**: The database layer caches vault files, stores vector embeddings (using local hardware-accelerated `nomic-embed-text-v1.5` in-process MLX/ONNX models), and maintains directed relationship edges (`relates_to` and `parent_of`) for graph traversal and semantic search.
3. **Lightweight Thin Clients (MCP & CLI)**: The command line interface and the Model Context Protocol (MCP) server run as thin HTTP clients forwarding all requests to a unified API gateway endpoint `/v1/mcp/call` on a persistent background daemon.
4. **Daemon Centrization**: The background daemon exclusively holds the database file write lock and coordinates the local in-process MLX model engines (embeddings and text completions). This design resolves lock contention, lowers individual client memory footprints by 50%, and makes query execution instantaneous.

```
                  ┌───────────────────────────────┐
                  │       Agent / IDE UI          │
                  └───────────────┬───────────────┘
                                  │ (HTTP POST /v1/mcp/call)
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
- **Resource Cleanup**: When the host IDE shuts down and terminates the MCP server process, the daemon continues running in the background to serve subsequent client requests, maintaining its warm local model (MLX/ORT) caches and RocksDB single-writer integrity.

---

## 2. Core Memory Entities

The database and vault manage three main entities:

### 2.1 Episode
- **Description**: Raw historical transcripts, conversation logs, and actions.
- **Storage Location**: `vault/episodes/antigravity_<session_id>_part<N>_<hash>.md`
- **Token Optimization**: Long logs are chunked at a maximum of `100,000` characters to prevent prompt bloat and local attention window failures.

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
- Chunks files into 24,000-token windows with 10% overlap (2,400 tokens).
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

The `mythrax` CLI is located in the `mythrax-core` directory. Commands automatically route requests to the daemon over HTTP on port `8090` using the security token found in `~/.mythrax/token`. If the daemon is inactive, the CLI automatically spawns the background daemon and polls for 15 seconds before executing the command.

| Group / Subcommand | Arguments | Purpose |
|--------------------|-----------|---------|
| `mythrax init` | `[harness]` | Bootstraps configuration folders, merge hooks, and auto-ingests transcripts. |
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

The MCP server exposes exactly **4 consolidated, action-enum-based tools** to client harnesses, cutting schema token overhead by over 75%.

### 8.1 `read`
- **Description**: Consolidated tool for all reading and querying operations.
- **Actions**:
  - `view`: Paginated view of text, PDF, video, or audio files on the local filesystem.
  - `search`: Semantic vector search on episodes, wiki nodes, and wisdom rules.
  - `rules`: Retrieves wisdom rules matching a target pattern.
  - `nodes`: Fetches specific database memory nodes by their IDs.
  - `root`: Returns the active Obsidian vault root path.
  - `query_symbolic`: Queries symbolic relations in the memory graph.
  - `search_index`: Queries or updates search indexing metadata.
  - `timeline`: Generates chronological memory timelines.
  - `get_full`: Retrieves full hydrated representation of memory nodes.
  - `get`: Gets local configuration settings or specific STM values.

### 8.2 `write`
- **Description**: Consolidated tool for all writing and modification operations.
- **Actions**:
  - `replace`: Replaces a contiguous block in a file.
  - `multi_replace`: Replaces multiple non-contiguous blocks in a file.
  - `save`: Saves an episodic memory to the vault and indexes it.
  - `feedback`: Records success or failure feedback for a wisdom rule.
  - `thought`: Records a temporal reasoning thought step.
  - `put`: Writes a short-term memory session key/value pair.
  - `clear`: Purges all short-term memory variables for a session.
  - `handoff`: Saves handoff contract metadata and STM state.
  - `set`: Sets active LLM provider configurations.

### 8.3 `manage`
- **Description**: Consolidated tool for all management, lifecycle, validation, reasoning (HTR), and ingestion operations.
- **Actions**:
  - `verify`: Checks database health and connection status.
  - `organize`: Organizes loose vault markdown files.
  - `reprocess`: Re-embeds or re-indexes vault files.
  - `summarize`: Generates high-level summaries of directories.
  - `audit`: Audits files for safety compliance.
  - `ingest_bulk`: Ingests conversation transcripts in bulk.
  - `ingest_forge`: Parses and chunks manuals/PDFs into structured knowledge.
  - `init`: Initializes root HTR node.
  - `ideate`: Generates child nodes or hypotheses for HTR.
  - `execute`: Spawns worktree, applies edits, and runs tests.
  - `backprop`: Backpropagates test results and insights up the tree.
  - `merge`: Performs HTR admission check and commits final changes.
  - `run`: Autonomously executes a multi-step HTR search cycle.
  - `pre_invocation`: Syncs belief states, context, and rules into STM.
  - `precompact`: Extracts wisdom rules and insights from conversation transcripts.
  - `audit_compliance`: Checks workspace configurations for safety.

### 8.4 `agent`
- **Description**: Consolidated tool for orchestrating local model autonomous task execution.
- **Actions**:
  - `complete_code_task`: Executes reasoning and coding tasks in-process.

---

## 9. Pre-Invocation & Pre-Compaction Hooks

To automate compliance checking, context preparation, and self-improvement loops, Mythrax features two primary hook entry points.

### 9.1 Pre-Invocation Hook
- **Purpose**: Runs immediately before the agent begins executing its turn. It checks code styles, audits the workspace directory for safety compliance, verifies the running memory daemon, and dynamically injects compiled context metadata nodes into the agent's short-term memory.
- **MCP Call**: Routed to the `manage` tool with the action `pre_invocation`.
- **How It Works**: The hook runner pings the daemon, checks directory states, and ensures the status block:
  ```markdown
  ### 🤖 Local Inference & Model Broker Status
  ```
  is formatted and injected cleanly into the agent context.

### 9.2 Pre-Compaction (Precompact) Hook
- **Purpose**: Triggered asynchronously after the agent completes a cycle or conversation session. It reads the raw log file or transcript path, runs extraction pipelines, distills new lessons/insights (as `WikiNode` or `WisdomRule` entities), writes them to the Obsidian vault, and updates the database.
- **MCP Call**: Routed to the `manage` tool with the action `precompact`.
- **API Endpoint**: Exposed via `POST /v1/hooks/precompact` on the core daemon:
  ```json
  {
    "session_id": "session_123",
    "transcript_path": "/absolute/path/to/transcript.jsonl"
  }
  ```

### 9.3 Installing the Hooks
Both hooks are registered automatically during initialization.

1. **Automatic Installation**:
   When you run the bootstrap command:
   ```bash
   mythrax init antigravity
   ```
   The CLI automatically reads the local CLI path and calls `merge_antigravity_hooks` to append the hook definitions to `~/.gemini/config/hooks.json`.

2. **Manual Configuration (hooks.json)**:
   Add the following JSON block to your `~/.gemini/config/hooks.json` under the `"mythrax-compliance"` key:
   ```json
   {
     "mythrax-compliance": {
       "PreInvocation": [
         {
           "type": "mcp",
           "server": "mythrax",
           "tool": "manage",
           "arguments": {
             "action": "pre_invocation"
           }
         }
       ]
     }
   }
   ```

---

## 10. Troubleshooting & Lock Resolution

### 10.1 Single-Writer Architecture
RocksDB requires an exclusive file lock on its database directory. Under the 2.x architecture, lock contention is avoided because **only the daemon process interacts with the database files**. All CLI commands and MCP server instances communicate with the daemon via HTTP on port `8090`. Multiple clients can run concurrently without encountering "Resource temporarily unavailable" lock errors.

### 10.2 Stale Processes & Force Unlocking
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
