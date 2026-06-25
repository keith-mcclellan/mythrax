# Test Plan - Mythrax 1.0 Release

This test plan defines the automated and manual verification procedures to validate that the client-server architecture, tool consolidation, and CLI refactoring function correctly and prevent RocksDB lock contention.

---

## Unit Tests

We will implement the following unit tests in `mythrax-core`:
- **Consolidated Router Validation (`mythrax-core/tests/test_mcp_routes.rs`)**:
  - Assert that a `tools/list` call returns exactly 9 tools with their correct schemas.
  - Assert that calling `query_memory` with action `search` correctly delegates to the search backend and yields results.
  - Assert that calling `manage_stm` with actions `put`/`get`/`clear` successfully manages short-term memory keys in SurrealDB.
  - Assert that invalid actions or arguments return clear, structured JSON-RPC errors.
- **Authentication Check**:
  - Test that the Axum routes reject requests with missing, malformed, or incorrect `X-Mythrax-Token` headers with a `401 Unauthorized` status.
  - Assert that correct tokens pass verification.

---

## Integration Tests

We will implement the following integration tests:
- **HTTP Proxy Roundtrip (`mythrax-core/tests/test_mcp_proxy.rs`)**:
  - Spin up a mock Axum server representing the daemon on an ephemeral port.
  - Run the refactored `McpServer` thin-client proxy.
  - Assert that JSON-RPC calls sent to the proxy over stdin are successfully translated to HTTP requests, sent to the mock server, and that the responses are piped back to stdout correctly.
- **Daemon Auto-Spawn Flow**:
  - With the daemon stopped and no process listening on port 8090, execute a CLI command or start the MCP proxy.
  - Assert that the client successfully spawns `mythrax daemon start` as a detached process.
  - Verify that the client waits for the daemon to initialize and successfully completes the command via HTTP.
  - Verify that `~/.mythrax/daemon.pid` contains the correct process ID.

---

## Acceptance Tests (Mapping to ACs)

### AC1: Concurrency and Lock Freedom
1. Start the daemon using `mythrax daemon start` in terminal window A.
2. In terminal window B, start the MCP proxy using `mythrax mcp` and send a `tools/list` request.
3. In terminal window C, run `mythrax memory query "test"`.
4. **Pass Criteria**: Both the MCP proxy and the CLI command must succeed immediately. The daemon must continue running watches andcompactions without showing any "RocksDB database locked" errors in the logs.

### AC2: Tool Consolidation Schema
1. Start the MCP server.
2. Send a `tools/list` JSON-RPC request over stdin.
3. **Pass Criteria**: The response must contain exactly 9 tool definitions matching our specification. The legacy 32 tools must no longer be advertised.

### AC3: CLI Client-Server & Auto-Spawn
1. Ensure the background daemon is completely stopped (verify no process is listening on port 8090 and `daemon.pid` does not exist).
2. Execute the command: `mythrax memory query "lock"`.
3. **Pass Criteria**: 
   - The CLI command must succeed and print search results.
   - The daemon must be running in the background.
   - `~/.mythrax/daemon.pid` must exist and match the running daemon process ID.
   - Subsequent calls to the CLI must execute instantly without spawning new daemon instances.

### AC4: Clean CLI Restructuring
1. Run `mythrax search --help`.
2. **Pass Criteria**: The command must fail with a Clap error: `error: Found argument 'search' which wasn't expected, or isn't valid in this context`.
3. Run `mythrax memory query --help`.
4. **Pass Criteria**: The command must succeed, displaying the help text for the new grouped memory query interface.

### AC5: Skill and Compliance Integration
1. Run an AI agent in the workspace and inspect its initial turn.
2. **Pass Criteria**:
   - The agent's first line of output must contain the compliance check: `Execution Check: [Karpathy Rules applied? Yes/No] [Local Model verified? Yes/No/Fallback]`.
   - The agent must successfully read the automatically injected pre-invocation hook output containing handoff metadata, STM variables, and negative constraints, and use that context to guide its edits.
