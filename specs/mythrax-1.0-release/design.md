# Design - Mythrax 1.0 Release

## Overview
The Mythrax 1.0 release establishes a strict **Client-Server Architecture** over localhost. The background daemon exclusively owns the database lock (RocksDB) and the heavy ONNX model runtime, while the MCP server and CLI commands act as lightweight HTTP clients.

### Architectural Layout

```mermaid
graph TD
    subgraph Client Processes
        CLI[mythrax memory query / htr / stm / ...]
        MCP[mythrax mcp - stdin/stdout]
    end

    subgraph Server Process (Single Writer)
        Daemon[mythrax daemon start - Port 8090]
        Surreal[SurrealDB / RocksDB Engine]
        ONNX[ONNX Embedding Model]
        Store[Obsidian Markdown Store]
    end

    CLI -->|HTTP + X-Mythrax-Token| Daemon
    MCP -->|HTTP + X-Mythrax-Token| Daemon
    Daemon --> Surreal
    Daemon --> ONNX
    Daemon --> Store
```

---

## Execution Flow

### 1. Client Initialization & Auto-Spawn Check
Whenever `mythrax mcp` or any client-mode CLI command is executed:
1. The client reads the security token from `~/.mythrax/token` and resolves the daemon address (default `http://127.0.0.1:8090`).
2. The client sends a lightweight HTTP ping (e.g. `GET /v1/config/llm` with the `X-Mythrax-Token` header) to check if the daemon is active.
3. **If Active**: The client proceeds directly to forwarding the request.
4. **If Inactive**:
   - The client spawns `mythrax daemon start` as a detached background child process.
   - The daemon process writes its process ID to `~/.mythrax/daemon.pid` and binds to port 8090, acquiring the exclusive RocksDB lock.
   - The client polls the daemon's ping endpoint every 200ms for up to 5 seconds.
   - If the port becomes active, the client proceeds. If the timeout expires, the client prints a clear error to stderr and exits with status `1`.

### 2. MCP Proxy Loop (`mythrax mcp`)
- The MCP proxy runs in an infinite loop reading JSON-RPC requests from `stdin`.
- For `tools/list`, it sends a `GET` request to `http://127.0.0.1:8090/v1/mcp/tools` and formats the returned list as a JSON-RPC response.
- For `tools/call`, it sends a `POST` request to `http://127.0.0.1:8090/v1/mcp/call` containing the tool name and arguments.
- It writes the HTTP responses back to `stdout` in JSON-RPC format.
- The proxy has **zero** database, file watcher, or ONNX model dependencies.

### 3. CLI Command Execution
- CLI subcommands parsed by Clap are mapped directly to their corresponding daemon REST endpoints or `/v1/mcp/call` payloads.
- For example, running `mythrax memory query "test" --limit 5` translates to a `POST /v1/mcp/call` request:
  ```json
  {
    "name": "query_memory",
    "arguments": {
      "action": "search",
      "query": "test",
      "limit": 5
    }
  }
  ```
- The CLI client receives the JSON response, formats it beautifully in the console, and exits.

---

## Interfaces

### Exposing MCP on Axum (`mythrax-core/src/api.rs` & `src/mcp_routes.rs`)
We will add two new routes to the daemon's Axum router:
1. `GET /v1/mcp/tools`:
   - Handled by `get_mcp_tools_handler`.
   - Returns a hardcoded JSON array containing the schema definitions for our 9 consolidated tools.
2. `POST /v1/mcp/call`:
   - Handled by `call_mcp_tool_handler`.
   - Takes a JSON payload: `{ name: String, arguments: Value }`.
   - Delegates execution to the consolidated routing logic in `src/mcp_routes.rs`.

---

## Refactoring Strategy & Code Structure

We will perform a surgical refactor across the following modules:

### 1. Consolidated Tool Routing (`mythrax-core/src/mcp_routes.rs`)
Create a new module containing the consolidated tool match arms. It will map the new consolidated action parameters back to their respective internal execution functions:
- `query_memory`: Calls `search_memories`, `search_wisdom`, `get_memory_nodes`, or returns the vault root.
- `record_memory`: Calls `save_episode` or `record_feedback`.
- `manage_htr`: Routes HTR actions (`init`, `ideate`, `execute`, `backprop`, `merge`, `run`).
- `manage_stm`: Routes STM actions (`put`, `get`, `clear`, `save_handoff`).
- `manage_vault`: Routes vault operations (`verify`, `organize`, `reprocess`, `summarize`).
- `manage_config`: Routes config get/set.
- `compliance_audit`: Routes workspace compliance auditing.
- `ingest_knowledge`: Routes bulk log ingestion and Forge file ingestion.
- `pre_invocation_hook`: Executes the pre-invocation context aggregation and negative constraint checks.

### 2. Extract Daemon Loop (`mythrax-core/src/daemon.rs`)
Extract the massive `tokio` background service loop from `main.rs` (which handles Obsidian file watching, HTTP server initialization, and background dream compactions) into a clean, dedicated `src/daemon.rs` module.

### 3. Decouple CLI Logic (`mythrax-core/src/vault/operations.rs`)
Move long operations (like rule conflict merging, database calibration auditing, and pre-commit hooks) out of `cli.rs` and into a new `src/vault/operations.rs` file. Update `cli.rs` to contain only the Clap parser definitions.

---

## Safety and Security Boundaries
1. **Localhost Binding**: The Axum server will bind strictly to `127.0.0.1`. It will refuse any external network interfaces, protecting the local memory vault from remote access.
2. **Constant-Time Token Comparison**: All REST endpoints (including the new `/v1/mcp/*` endpoints) will enforce constant-time token validation against `~/.mythrax/token` via `crate::auth::verify_token_constant_time` to prevent timing attacks.
3. **Secret Filtering**: All STM session variables and episodes will continue to run through the `SecretFilter` to scrub API keys, SSH keys, and passwords before writing to the Obsidian markdown files on disk.

---

## Observability
- When the daemon is spawned in the background by the client, it will redirect its `stdout` and `stderr` to `~/.mythrax/daemon.log`.
- This ensures that if the daemon fails to start or encounters internal errors, the developer can easily inspect `tail -f ~/.mythrax/daemon.log` for debugging.
- The CLI client will print clean, user-friendly error messages if the HTTP connection fails, rather than dumping raw connection panic stack traces.
