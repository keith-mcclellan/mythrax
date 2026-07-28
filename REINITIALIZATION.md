# ⚔️ Mythrax 3.0 Reinitialization & Ingestion Playbook

This document provides concise, authoritative instructions for a new agent context to clean the environment, reinitialize the Mythrax daemon, execute batched document ingestion, verify workspace doc vault mirroring, and delegate continuous background dreaming to the **Cloud Brain** subagent using **Mythrax MCP Endpoints** (`read`, `write`, `manage`, `agent`) as the primary interface.

---

## 🛡️ Architecture & Memory Safety Invariants

The Mythrax 3.0 engine incorporates 11 strict memory safety invariants to guarantee zero OOM crashes, zero socket leaks, zero unbounded graph edge bloat, and fast query response times:

1. **Paginated Query Contract (`LIMIT 50 START $offset`)**: All episode, wisdom rule, wiki node, and handoff queries use 50-item paginated result windows, eliminating un-paginated database loads into RAM.
2. **SHA-256 Content-Hash Deduplication (`idx_content_hash`)**: Episodes, wisdom rules, and `WikiNode` records compute structural SHA-256 hashes for $O(1)$ duplicate checking, preventing duplicate memory inflation.
3. **Atomic Single-Transaction Edge & Metrics Cascading**: All node deletion routines (`delete_episode`, `delete_wiki_node`, `delete_by_vault_path`, `delete_stale_handoffs`, compactor GC, graduation LRU) execute inside single atomic `BEGIN TRANSACTION; ... COMMIT TRANSACTION;` blocks, cascading deletions across all 4 relation tables (`relates_to`, `followed_by`, `mentions`, `superseded_by`) and the `metrics` table (`DELETE metrics WHERE target_id = $id;`).
4. **Workspace & Project Doc Vault Mirroring (`sync_workspace_docs_to_vault`)**: Workspace-root documents (`ARCHITECTURE.md`, `REINITIALIZATION.md`, `conductor/tracks/**/*.md`, `specs/**/*.md`) are automatically mirrored into `vault_root/reference/` with cross-platform forward-slash (`/`) path normalization, SHA-256 diffing, and atomic `MOC.md` rebuilding.
5. **Watcher Event Suppression**: File events for `/reference/` paths, `MOC.md`, and `*.tmp` files are ignored by `vault::watcher` to prevent cascading background LLM dreaming passes.
6. **Bounded Graph Expansion**: BFS graph traversals (`query_symbolic_scored_db`) are capped at 1,000 hits with constant-time $O(1)$ `HashMap<String, usize>` index lookups.
7. **Sliding Window Transcript Caps**: Transcript mining tool sequences are capped at a 1,000-element sliding window (`VecDeque`), preventing memory leaks during large transcript parses.
8. **HTTP Client Socket Reuse**: All HTTP proxy routes and RPC calls reuse a process-global static `reqwest::Client` connection pool, preventing file descriptor (`EMFILE`) socket exhaustion.

---

## 🧹 Environment Cleanup & Reset

Prefer MCP endpoints for maintenance cleaning. Use shell commands ONLY for initial process execution or full destructive disk resets.

### Option A: Maintenance Clean via MCP Endpoint (Recommended)
Cleans up stale sessions, expired short-term memory files, `.trash/` files, and orphaned HTR branches from SurrealDB and the Obsidian vault via the `manage` MCP tool:

* **MCP Tool Call**:
  ```json
  {
    "ServerName": "mythrax",
    "ToolName": "manage",
    "Arguments": {
      "action": "clean"
    }
  }
  ```

* **Or via `manage(action="verify")` to repair schemas & links**:
  ```json
  {
    "ServerName": "mythrax",
    "ToolName": "manage",
    "Arguments": {
      "action": "verify",
      "fix": true
    }
  }
  ```

### Option B: Full Destructive Reset (Fresh Slate - CLI Fallback)
Wipes the entire SurrealDB local database and clears local Obsidian vault directories:
```bash
# 1. Stop the daemon if running
mythrax daemon stop

# 2. Delete SurrealDB local database directory and configs
rm -rf ~/.mythrax/db/ ~/.mythrax/data/ ~/.mythrax/config.json

# 3. Clean local Obsidian vault directories
rm -rf ~/mythrax-vault/.trash/
rm -rf ~/mythrax-vault/.handoffs/
rm -rf ~/mythrax-vault/sessions/
rm -rf ~/mythrax-vault/wiki/
rm -rf ~/mythrax-vault/episodes/
rm -rf ~/mythrax-vault/wisdom/
rm -rf ~/mythrax-vault/reference/
```

---

## 🛠️ Step-by-Step Reinitialization via MCP & Service Lifecycles

### 1. Process Launch (Metal Environment & Daemon Start)
On macOS, export the Xcode developer directory before starting the background daemon process:
```bash
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
mythrax daemon start
```

### 2. Vault Subdirectory & System Bootstrap via MCP Endpoint
Initialize configuration, vault subdirectories, and system schemas using the `manage(action="bootstrap")` MCP tool:

* **MCP Tool Call**:
  ```json
  {
    "ServerName": "mythrax",
    "ToolName": "manage",
    "Arguments": {
      "action": "bootstrap",
      "scope": "general"
    }
  }
  ```

### 3. Chronological Bulk Ingestion via MCP Endpoint
To prevent VRAM exhaustion and watchdog GPU hangs, run bulk ingestion in batches using the `manage(action="ingest_bulk")` MCP tool:

* **MCP Tool Call**:
  ```json
  {
    "ServerName": "mythrax",
    "ToolName": "manage",
    "Arguments": {
      "action": "ingest_bulk",
      "source": "/Users/keith/.gemini/antigravity/brain",
      "harness": "antigravity",
      "scope": "general"
    }
  }
  ```

---

## 🧠 Cloud Brain Delegation Playbook via MCP

For dreaming, distillation, and continuous cognitive callback resolution, delegate background work to the **Cloud Brain** subagent via `invoke_subagent`.

### Spawning the Subagent
Invoke a `self` type subagent with the role `Mythrax Cloud Brain` and inherit workspace permissions. Use this exact prompt:

```text
You are the Mythrax Cloud Brain. Your job is to orchestrate the bootstrap, dreaming, and cognitive callback resolution for Mythrax using MCP endpoints.

IMPORTANT RULES:
1. CRITICAL SYNTAX: In your call_mcp_tool calls, the ServerName argument MUST be the exact, unquoted, bare string mythrax, and the ToolName argument MUST be the exact, unquoted, bare string manage or write. Do NOT use escaped quotes like "\"mythrax\"" or add any quotes inside the JSON string arguments. This is a strict syntax requirement. If you fail to do this, the system will deadlock.
2. You MUST ONLY use call_mcp_tool endpoints to query, summarize, and resolve callbacks.
3. You MUST NEVER run any daemon start, stop, kill, or other command-line tools under any circumstances.
4. If the daemon is unreachable (returns EOF or connection closed), simply wait and try again.
5. Run in an infinite loop. Even if the pre_invocation check returns 0 pending callbacks, DO NOT stop or exit the loop. Sleep/wait for 5 seconds and query again. You must run continuously in this check-sleep cycle forever until explicitly terminated by the parent.

Please execute the following sequence:
1. PHASE 1: Bootstrap & Embedding Catch-Up via MCP
   - Call call_mcp_tool with ServerName="mythrax", ToolName="manage", Arguments={action: "bootstrap", force: true, async_mode: true}.
   - Call call_mcp_tool with ServerName="mythrax", ToolName="manage", Arguments={action: "reprocess"}.

2. PHASE 2: Dynamic Scope Dreaming & Direction Ideation Trigger via MCP
   - Call call_mcp_tool with ServerName="mythrax", ToolName="read", Arguments={action: "scopes"} to retrieve all active scopes across SurrealDB and the Obsidian vault.
   - To prevent macOS Metal GPU timeout/hang crashes, trigger summarization and direction ideation for EACH discovered scope SEQUENTIALLY (waiting for each MCP tool call to return before starting the next) with async_mode=false:
     For each scope in discovered_scopes:
       - Call call_mcp_tool with ServerName="mythrax", ToolName="manage", Arguments={action: "summarize", scope: "<scope_name>", async_mode: false}.
       - Call call_mcp_tool with ServerName="mythrax", ToolName="manage", Arguments={action: "ideate", scope: "<scope_name>", hypothesis: "Auto-synthesize research directions for scope"}.

3. PHASE 3: Wisdom Graduation & Vault Organization via MCP
   - Call call_mcp_tool with ServerName="mythrax", ToolName="manage", Arguments={action: "audit_compliance"}.
   - Call call_mcp_tool with ServerName="mythrax", ToolName="manage", Arguments={action: "organize"}.

4. PHASE 4: Continuous Callback Resolution Loop via MCP
   Run in an infinite loop:
     - Call call_mcp_tool with ServerName="mythrax", ToolName="manage", Arguments={session_id: "<active_session_id>", action: "pre_invocation", caller: "distiller"}.
     - Read the output. Look for the section '### 🧠 Pending Cognitive Callbacks'.
     - For each task in that section:
       - Extract the Callback ID, system instruction, and prompt.
       - Using your cloud brain, generate the output.
       - Call call_mcp_tool with ServerName="mythrax", ToolName="write", Arguments={action: "cognitive_callback", callback_id: "<Callback ID>", result: "<your generated output as a string>"}.
     - Sleep/wait for 5 seconds and repeat.
```

---

## 💡 Verification & Inspection via MCP Endpoints

Prefer MCP `read` and `manage` tools to verify system state:

1. **Verify Vault Root**:
   * MCP Tool: `read(action="root")`
2. **Search Mirrored Workspace Reference Docs**:
   * MCP Tool: `read(action="search", query="ARCHITECTURE.md", scope="workspace_ref")`
3. **Verify Active Wisdom Rules**:
   * MCP Tool: `read(action="rules", query="rust")`
4. **Query Graph Relations**:
   * MCP Tool: `read(action="query_symbolic", node_id="wiki_node:<id>", max_depth=2)`
