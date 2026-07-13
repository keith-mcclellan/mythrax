# ⚔️ Mythrax 3.0 Reinitialization & Ingestion Playbook

This document provides concise instructions for a new agent context to quickly clean the environment, reinitialize the Mythrax daemon, and reingest the historical Antigravity conversations.

---

## 🧹 Environment Cleanup & Reset

Before reinitializing, choose either a **Maintenance Clean** (preserves existing memories) or a **Full Destructive Reset** (wipes everything).

### Option A: Maintenance Clean (Safe)
Cleans up stale sessions, expired short-term memory files, `.trash/` files, and orphaned HTR branches from the Obsidian vault and SurrealDB database without deleting actual memories.

*   **CLI Option**:
    ```bash
    mythrax vault clean --confirm
    ```
*   **MCP Tool Call Option (For Agents)**:
    Call the `manage` tool with the `clean` action and `confirm = true`:
    ```json
    {
      "name": "manage",
      "arguments": {
        "action": "clean",
        "confirm": true
      }
    }
    ```

### Option B: Full Destructive Reset (Fresh Slate)
Wipes the entire SurrealDB database and clears the local Obsidian vault directory:
```bash
# 1. Stop the daemon if running
mythrax daemon stop

# 2. Delete the SurrealDB local database directory
rm -rf ~/.mythrax/data/

# 3. Clean the Obsidian vault directories
rm -rf ~/mythrax-vault/.trash/
rm -rf ~/mythrax-vault/.handoffs/
rm -rf ~/mythrax-vault/sessions/
```

---

## 🛠️ Step-by-Step Reinitialization

### 1. Initialize Configuration & Hooks
Run the initialization CLI command targeting the `antigravity` harness:
```bash
mythrax init antigravity
```

### 2. Start the Daemon
Spawn the central memory sidecar daemon in the background:
```bash
mythrax daemon start
```

### 3. Bootstrap and Ingest Historical Conversations
Reingest all historical Antigravity conversations and distill them:

*   **CLI Option**:
    Specifying `--distill-model cloud` ensures that the cloud model is leveraged for all distillation and summarization tasks:
    ```bash
    mythrax bootstrap --distill-model cloud
    ```
*   **MCP Tool Call Option (For Agents)**:
    Call the `manage` tool with the `bootstrap` action. The daemon's internal model router will automatically assign the appropriate high-tier (cloud) task profile to perform the distillation:
    ```json
    {
      "name": "manage",
      "arguments": {
        "action": "bootstrap"
      }
    }
    ```

---

## 💡 Notes for the Agent Context
* **Incremental Processing**: By default, the `bootstrap` command runs incrementally and skips already processed files. To force a full re-processing, append the `force = true` parameter (MCP) or `--force` flag (CLI).
* **Verification**: Verify that the ingestion completed and nodes are successfully registered in the database by querying the memory:
  *   **CLI Option**:
      ```bash
      mythrax memory query "model routing"
      ```
  *   **MCP Option**:
      ```json
      {
        "name": "read",
        "arguments": {
          "action": "search",
          "query": "model routing"
        }
      }
      ```
