# ⚔️ Mythrax 3.0 Reinitialization & Ingestion Playbook

This document provides concise instructions for a new agent context to quickly clean the environment, reinitialize the Mythrax daemon, run batched chronological ingestion, and spawn the persistent callback resolution loop.

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

## 🧠 Cloud Brain Subagent Playbook

For dreaming, distillation, and continuous callback resolution, you must spawn a persistent subagent.

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
* **Verification**: Verify that the database nodes and compactions are successfully registered by querying the vault:
  ```bash
  find ~/mythrax-vault/wiki/ -path '*/compactions/*' -type f
  find ~/mythrax-vault/wisdom/ -type f
  ```
