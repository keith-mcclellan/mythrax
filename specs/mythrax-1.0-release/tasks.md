# Tasks - Mythrax 1.0 Release

This task list breaks down the implementation into small, surgical, verifiable steps ordered by dependency.

---

## T1: Implement Consolidated MCP Routes & REST Endpoints
- **Purpose**: Expose the consolidated MCP tools schema and tool execution handler on the background daemon's REST API.
- **Related Requirements**: AC2 (Tool Consolidation)
- **Related Tests**: `test_mcp_routes.rs` (Unit tests)
- **Inputs**: None.
- **Actions**:
  1. Create `mythrax-core/src/mcp_routes.rs`.
  2. Implement the schemas for the 9 consolidated tools: `query_memory`, `record_memory`, `manage_htr`, `manage_stm`, `manage_vault`, `manage_config`, `compliance_audit`, `ingest_knowledge`, `pre_invocation_hook`.
  3. Implement the POST router that parses the tool name and `action` enum and delegates to the backend.
  4. In `mythrax-core/src/api.rs`, wire up `GET /v1/mcp/tools` and `POST /v1/mcp/call`.
- **Expected Output**: Running the daemon exposes these two endpoints, protected by the security token.
- **Validation**: Perform `curl -H "X-Mythrax-Token: <token>" http://127.0.0.1:8090/v1/mcp/tools` and verify exactly 9 tools are returned.

---

## T2: Refactor MCP Server to Thin-Client HTTP Proxy & Auto-Spawn
- **Purpose**: Refactor the MCP server process to act as a lightweight gateway that forwards JSON-RPC requests over HTTP to the daemon, automatically spawning the daemon if it is stopped.
- **Related Requirements**: AC1 (Lock Freedom), AC3 (Auto-Spawn)
- **Related Tests**: `test_mcp_proxy.rs` (Integration tests)
- **Inputs**: `mcp.rs`
- **Actions**:
  1. Modify `mythrax-core/src/mcp.rs` to strip out SurrealDB, RocksDB, and MarkdownStore dependencies.
  2. Implement the HTTP forwarding logic using `reqwest` or `ureq` to send stdin requests to the daemon's Axum endpoints.
  3. Implement the auto-spawn function: if port 8090 is inactive, spawn `mythrax daemon start` in a detached child process, write the PID to `~/.mythrax/daemon.pid`, and poll the ping endpoint every 200ms for up to 5 seconds.
- **Expected Output**: `mythrax mcp` runs as a zero-dependency process that forwards requests.
- **Validation**: Launch `mythrax mcp` with the daemon stopped. Verify the daemon is automatically started and the MCP handshake completes successfully.

---

## T3: CLI Restructuring & Legacy Command Removal
- **Purpose**: Clean up the CLI Clap parser definitions to completely remove legacy top-level commands and organize subcommands under nested namespaces.
- **Related Requirements**: AC4 (Clean CLI)
- **Related Tests**: Manual CLI help validation
- **Inputs**: `cli.rs`
- **Actions**:
  1. Remove legacy top-level subcommands: `Search`, `Save`, `Verify`, `Forge` from the clap enum in `cli.rs`.
  2. Define the new nested subcommand enums: `Memory`, `Htr`, `Stm`, `Vault`, `Config`, `Audit`, `Ingest`.
  3. Implement a custom help command that prints a clean guide explaining the new nested layout to the user.
- **Expected Output**: A clean, grouped CLI parser.
- **Validation**: Run `mythrax search` and assert it fails. Run `mythrax memory query --help` and verify it succeeds.

---

## T4: Implement CLI Client-Server HTTP Forwarding & Spawning
- **Purpose**: Update the CLI execution handlers in `main.rs` to run as HTTP clients pointing to the daemon REST API rather than opening RocksDB directly, using the same auto-spawn mechanism.
- **Related Requirements**: AC1 (Lock Freedom), AC3 (Auto-Spawn)
- **Related Tests**: `test_mcp_proxy.rs` (Auto-spawn flow)
- **Inputs**: `main.rs`, `cli.rs`
- **Actions**:
  1. In `main.rs`, implement the client forwarding layer for all CLI subcommands.
  2. Read the token from `~/.mythrax/token` and attach it as `X-Mythrax-Token` on all requests.
  3. Implement the HTTP check and auto-spawn check (sharing the same helper function as the MCP proxy).
  4. Forward command arguments to the daemon's API and format the JSON response.
- **Expected Output**: The CLI binary runs as a lightweight HTTP client.
- **Validation**: With the daemon running, execute `mythrax memory query "test"` and verify it returns results successfully and immediately without RocksDB lock crashes.

---

## T5: Unified Skill and Pre-Invocation Hook Documentation
- **Purpose**: Update active skill files to document the new consolidated tools and explicitly instruct agents on pre-invocation compliance.
- **Related Requirements**: AC5 (Skill Integration)
- **Related Tests**: Agent invocation validation
- **Inputs**: `.agents/skills/mythrax/SKILL.md`, `/Users/keith/.gemini/config/skills/mythrax/SKILL.md`
- **Actions**:
  1. Replace the legacy 32 tools reference table in `SKILL.md` with the 9 new consolidated tools.
  2. Add a prominent section detailing the `pre_invocation_hook` output processing rules and memory query compliance checks.
  3. Replicate the changes to the global config skill copy.
- **Expected Output**: A clean, unified, high-cohesion skill guide.
- **Validation**: Verify that opening the skill file shows the correct consolidated tool reference.

---

## T6: Structural Refactoring & Code Debt Cleanup
- **Purpose**: Extract bloated loops and operations out of `main.rs` and `cli.rs` into dedicated modules to meet 1.0 production standards.
- **Related Requirements**: Structural extraction and refactoring
- **Related Tests**: `cargo test` compilation and execution
- **Inputs**: `main.rs`, `cli.rs`
- **Actions**:
  1. Create `mythrax-core/src/daemon.rs` and move the background daemon watch and Axum server tokio runtime out of `main.rs`.
  2. Move `handle_merge_vault` and `run_auditor` from `cli.rs` to a new `src/vault/operations.rs` module.
  3. Add standard `///` Rustdoc to all exported functions in the new modules.
- **Expected Output**: A highly organized, modularized, and documented Rust codebase.
- **Validation**: Run `cargo test` and verify that the codebase compiles cleanly without warnings or errors.
