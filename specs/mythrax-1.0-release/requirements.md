# Requirements - Mythrax 1.0 Release

## Problem
Currently, the Mythrax system experiences several blockers that prevent it from being a cohesive 1.0 product:
1. **Concurrency Blocker**: RocksDB mandates an exclusive process lock. Because both the background daemon (`mythrax daemon start`) and the MCP server (`mythrax mcp`) try to open the database directly, they cannot run at the same time. When an agent runs, background dreaming, compactions, and watches are halted, or the tool calls crash.
2. **CLI Blocker**: Similarly, executing any manual CLI command (like `mythrax search`) while the daemon is running results in database lock crashes, frustrating the user and disrupting workflows.
3. **Context Schema Bloat**: Advertising 32 granular tools to the agent consumes thousands of context tokens on every interaction turn. This causes slow response times, high token costs, and increases the likelihood of the LLM picking the wrong tool.
4. **Agent Memory Query Compliance**: Agents frequently bypass or ignore the pre-invocation memory hook because there is no explicit instruction in their active skill documentation telling them how to consume the injected context or enforcing compliance checks.

## Outcome
A single, unified, client-server architecture where:
- The background daemon is the **single writer** that exclusively owns the RocksDB database lock and ONNX embedding models.
- The MCP server and CLI commands act as **lightweight HTTP clients** that securely communicate with the daemon over localhost.
- The MCP tools are **consolidated from 32 to 9** high-level tools, cutting context schema bloat by >60%.
- The CLI is restructured into clean, nested namespaces matching these consolidated tools, with all legacy top-level commands completely removed for a lean, high-fidelity codebase.
- The `mythrax` skill is updated to teach agents about the consolidated tools and enforce pre-invocation compliance.

## User Value
- **Zero Lock Contention**: Users and agents can execute CLI commands, query memory, and run background dreaming tasks simultaneously without any database crashes.
- **Fast Startup**: MCP and CLI commands execute virtually instantaneously because they no longer incur the overhead of opening RocksDB or loading the heavy ONNX embedding models.
- **High Token Efficiency**: Consolidated tool schemas consume significantly fewer context tokens, making local model inference faster and cheaper.
- **Guaranteed Context Recall**: The pre-invocation hook is highly reliable, ensuring agents always align with past work and active rules before writing code.

## In Scope
1. **Daemon REST API Extension**:
   - Expose `/v1/mcp/tools` (GET) to return the consolidated MCP tool schemas.
   - Expose `/v1/mcp/call` (POST) to accept a JSON-RPC-like tool call and execute it using the daemon's internal backend.
2. **Lightweight MCP Proxy**:
   - Refactor `mythrax mcp` to be a pure proxy, translating stdin/stdout JSON-RPC requests to HTTP calls against the daemon.
   - Implement background daemon auto-spawning (with PID file checking and port-ready polling) if the daemon is inactive.
3. **CLI Client-Server Refactoring**:
   - Restructure the CLI subcommands under grouped namespaces: `memory`, `htr`, `stm`, `vault`, `config`, `audit`, `ingest`.
   - Completely remove the legacy top-level commands: `search`, `save`, `verify`, `forge`.
   - Update `main.rs` CLI handlers to forward all commands to the daemon's REST API or `/v1/mcp/call` using HTTP client requests authenticated with `~/.mythrax/token`.
   - Implement the same daemon auto-spawning mechanism in the CLI client.
4. **Unified Mythrax Skill Update**:
   - Revise `.agents/skills/mythrax/SKILL.md` and its global counterpart to document the 9 consolidated tools and the pre-invocation hook guidelines.

## Out of Scope
- Enabling remote networks or public IPs to connect to the daemon (must remain strictly bound to `127.0.0.1` for security).
- Changing or replacing SurrealDB or RocksDB as the storage engines.
- Refactoring internal database schemas or rules structures.

## Inputs and Outputs

### 1. GET `/v1/mcp/tools`
- **Request Headers**: `X-Mythrax-Token: <token>`
- **Response**: JSON array of exactly 9 consolidated MCP tool schemas.

### 2. POST `/v1/mcp/call`
- **Request Headers**: `X-Mythrax-Token: <token>`
- **Request Body**:
  ```json
  {
    "name": "query_memory",
    "arguments": {
      "action": "search",
      "query": "surrealdb locking",
      "scope": "mythrax",
      "limit": 5
    }
  }
  ```
- **Response**: JSON-RPC compatible result wrapper.

## Constraints & Assumptions
- The security token is located in `~/.mythrax/token`.
- The daemon binds to `127.0.0.1:8090` by default.
- The project is pre-1.0, meaning breaking CLI and MCP changes are fully acceptable.
- Spawning a background process in Rust must be handled via `std::process::Command` without blocking the parent client thread.

## Risks and Edge Cases
- **Daemon Startup Latency**: Spawning the daemon takes a brief moment to initialize RocksDB. The client must poll the port (up to 5 seconds) before failing.
- **Port Conflicts**: If port 8090 is occupied by another application, the daemon will fail to bind. We must log a clear error message.
- **Stale PID Files**: If the daemon crashed previously, a stale PID file might exist. The auto-spawn logic must verify if the process ID is actually active before assuming the daemon is running.

## Acceptance Criteria
- **AC1: Concurrency and Lock Freedom**
  - The background daemon is running (`mythrax daemon start`).
  - Running `mythrax mcp` or running CLI commands (like `mythrax memory query "lock"`) succeeds immediately and does not trigger any RocksDB process lock errors.
- **AC2: Tool Consolidation**
  - Running a `tools/list` request against the MCP server returns exactly 9 consolidated tools: `query_memory`, `record_memory`, `manage_htr`, `manage_stm`, `manage_vault`, `manage_config`, `compliance_audit`, `ingest_knowledge`, and `pre_invocation_hook`.
- **AC3: CLI Client-Server & Auto-Spawn**
  - With the daemon stopped, running `mythrax memory query "test"` automatically spawns `mythrax daemon start` in the background, waits for it to become healthy, and successfully returns the query results via HTTP.
  - The spawned daemon persists in the background and writes its PID to `~/.mythrax/daemon.pid`.
- **AC4: Clean CLI Restructuring**
  - Legacy commands `mythrax search`, `mythrax save`, `mythrax verify`, and `mythrax forge` are completely removed from the binary. Executing them fails with standard Clap command-not-found help.
  - All operations are successfully executed through the new grouped subcommands.
- **AC5: Skill and Compliance Integration**
  - The `.agents/skills/mythrax/SKILL.md` contains updated descriptions of the 9 consolidated tools and explicitly details how agents must process and comply with the pre-invocation memory hook.
