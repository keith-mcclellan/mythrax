# ⚔️ Mythrax 3.0 Reinitialization & Ingestion Playbook

This document provides concise, authoritative instructions for a new agent context to clean the environment, reinitialize the Mythrax daemon, run batched chronological ingestion, manage workspace doc mirroring, and delegate continuous background dreaming to the **Cloud Brain** subagent.

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

Before reinitializing, choose either a **Maintenance Clean** (preserves existing memories) or a **Full Destructive Reset** (wipes everything).

### Option A: Maintenance Clean (Safe)
Cleans up stale sessions, expired short-term memory files, `.trash/` files, and orphaned HTR branches from the Obsidian vault and SurrealDB database without deleting actual memories.

```bash
mythrax vault clean --confirm
```

### Option B: Full Destructive Reset (Fresh Slate)
Wipes the entire SurrealDB database and clears the local Obsidian vault directory:
```bash
# 1. Stop the daemon if running
mythrax daemon stop

# 2. Delete the SurrealDB local database directory and configs
rm -rf ~/.mythrax/db/ ~/.mythrax/data/ ~/.mythrax/config.json

# 3. Clean the Obsidian vault directories
rm -rf ~/mythrax-vault/.trash/
rm -rf ~/mythrax-vault/.handoffs/
rm -rf ~/mythrax-vault/sessions/
rm -rf ~/mythrax-vault/wiki/
rm -rf ~/mythrax-vault/episodes/
rm -rf ~/mythrax-vault/wisdom/
rm -rf ~/mythrax-vault/reference/
```

---

## 🛠️ Step-by-Step Reinitialization

### 1. Initialize Configuration & Vault Subdirectories
Run the initialization CLI command targeting the `antigravity` harness (this step runs instantly and configures files without executing ingestion):
```bash
mythrax init antigravity
```

### 2. Export Metal Environment & Start Daemon
On macOS, you **MUST** export the Xcode developer directory before starting the daemon. This prevents the Metal JIT compiler from crashing with command buffer timeout/hang errors during embedding generation:
```bash
export DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer
mythrax daemon start
```

### 3. Chronological Batch Ingestion
To prevent VRAM exhaustion and watchdog GPU hangs, run bulk ingestion in batches of **50 conversations** at a time:
```bash
mythrax ingest --source /Users/keith/.gemini/antigravity/brain --harness antigravity --batch-size 50
```
This CLI command reads the source directory, sorts conversations chronologically, and sends them sequentially to the running daemon via the MCP HTTP API.

---

## 🧠 Cloud Brain Delegation Playbook

For dreaming, distillation, and continuous cognitive callback resolution, delegate background work to the **Cloud Brain** subagent via `invoke_subagent`.

### Spawning the Subagent
Invoke a `self` type subagent with the role `Mythrax Cloud Brain` and inherit workspace permissions. Use this exact prompt:

```text
You are the Mythrax Cloud Brain. Your job is to orchestrate the bootstrap, dreaming, and cognitive callback resolution for Mythrax.

IMPORTANT RULES:
1. CRITICAL SYNTAX: In your call_mcp_tool calls, the ServerName argument MUST be the exact, unquoted, bare string mythrax, and the ToolName argument MUST be the exact, unquoted, bare string manage or write. Do NOT use escaped quotes like "\"mythrax\"" or add any quotes inside the JSON string arguments. This is a strict syntax requirement. If you fail to do this, the system will deadlock.
2. You MUST ONLY use the call_mcp_tool to query and write callbacks.
3. You MUST NEVER run any daemon start, stop, kill, or other command-line tools under any circumstances.
4. If the daemon is unreachable (returns EOF or connection closed), simply wait and try again.
5. Run in an infinite loop. Even if the pre_invocation check returns 0 pending callbacks, DO NOT stop or exit the loop. Sleep/wait for 5 seconds and query again. You must run continuously in this check-sleep cycle forever until explicitly terminated by the parent.

Please execute the following sequence:
1. PHASE 1: Bootstrap
   Call call_mcp_tool with ServerName="mythrax", ToolName="manage", Arguments={action: "bootstrap", force: true, async_mode: true}.
   
2. PHASE 2: Sequential Dreaming Trigger
   To prevent macOS Metal GPU timeout/hang crashes, trigger summarization for each scope SEQUENTIALLY (waiting for each MCP tool call to return before starting the next) with async_mode=false:
     For each scope in ["general", "mythrax", "smwl", "social-experiment", "repos", "obsidian-knowledge-graph"]:
       Call call_mcp_tool with ServerName="mythrax", ToolName="manage", Arguments={action: "summarize", scope: "<scope_name>", async_mode: false}.
       
3. PHASE 3: Continuous Callback Resolution Loop
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

## 💡 Notes & Verification
* **Tighter Thresholds**: Embedding matching distance for centroids is set to `0.10` (cosine distance) to prevent high-similarity transcript logs from blending into single oversized insights.
* **Workspace Doc Mirroring Verification**: Verify that workspace markdown files are automatically mirrored into the vault `reference/` directory and linked in `MOC.md`:
  ```bash
  ls -la ~/mythrax-vault/reference/
  cat ~/mythrax-vault/MOC.md
  ```
* **Memory Node Verification**: Verify that database nodes and compactions are successfully registered by querying the vault:
  ```bash
  find ~/mythrax-vault/wiki/ -path '*/compactions/*' -type f
  find ~/mythrax-vault/wisdom/ -type f
  ```
