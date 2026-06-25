# Clarify - Mythrax 1.0 Release

## Restated Request
We need to prepare the Mythrax codebase for a production-grade 1.0 launch by resolving process lock contention, improving token efficiency, aligning interfaces, and enforcing memory querying. Specifically:
1. **Resolve RocksDB Lock Contention**: Enable the daemon and MCP/CLI processes to run concurrently by refactoring the MCP server and CLI into lightweight HTTP clients that forward all database and tool operations to a single, central background daemon.
2. **Consolidate MCP Tools**: Collapse the 32 granular MCP tools down to 9 high-level, action-enum-based tools. This will reduce LLM context schema bloat by >60% and improve agent tool-selection accuracy.
3. **Align and Refactor CLI (Pre-1.0 Breaking Changes)**: Group CLI subcommands into nested namespaces matching the new consolidated MCP tools. Completely remove legacy top-level commands (`search`, `save`, `verify`, `forge`) to avoid codebase bloat.
4. **Lightweight CLI client mode**: Enable CLI commands to automatically forward requests over HTTP to the daemon using the token stored in `~/.mythrax/token`. If the daemon is inactive, auto-spawn it in the background and wait up to 5 seconds.
5. **Unified Mythrax Skill & Pre-Invocation Integration**: Update the `mythrax` skill (`.agents/skills/mythrax/SKILL.md` and the global copy) to document the 9 consolidated tools and integrate the pre-invocation hook guidelines, instructing agents to verify injected hook context and query memory before doing work.

## Known Facts
- **Exclusive Lock**: RocksDB enforces a strict single-process write lock. Multiple concurrent instances attempting to open `rocksdb://...` will fail.
- **Daemon Address**: The daemon runs an Axum-based REST server on `http://127.0.0.1:8090` by default.
- **Token Security**: The security token is generated during `mythrax init` and written to `~/.mythrax/token` (default path). The daemon expects this token in the `X-Mythrax-Token` HTTP header.
- **MCP Server**: The MCP server currently runs over stdin/stdout, and is started via `mythrax mcp`.
- **Pre-Invocation Hook**: Defined in `hooks.json` to automatically run `verify_compliance` and `pre_invocation_hook` before every agent turn.

## Assumptions
- **Local Host Focus**: The daemon runs locally on `127.0.0.1`, which minimizes latency overhead for client-server HTTP requests.
- **Unix Environment**: The OS is macOS/Linux, meaning daemon auto-spawning can be reliably implemented by spawning the background process, writing the PID file, and verifying port binding.
- **Pre-1.0 Breaking Changes**: Breaking changes to the CLI and MCP signatures are fully permitted, allowing us to purge legacy commands immediately rather than carrying deprecated alias routing bloat.

## Ambiguities
*All major ambiguities have been resolved through the `/grill-me` alignment:*
- **RocksDB Fallback**: The CLI will *not* fall back to direct RocksDB access if the daemon is stopped. Instead, it will auto-spawn the daemon in the background, ensuring RocksDB is only ever opened by a single daemon process.
- **Token Auth**: The CLI will read the token from `~/.mythrax/token` and attach it as the `X-Mythrax-Token` header, matching the daemon's existing authentication model.
- **Legacy Commands**: Legacy top-level subcommands are completely removed. Clean Clap descriptions and help texts will guide the user to the new grouped namespaces.

## Tradeoffs
- **Complexity Shift**: We transfer database, LLM, and model complexity from the MCP/CLI binaries to the daemon. The MCP and CLI become simple HTTP wrappers. This makes the client-side execution extremely fast and lightweight, while centralizing state management.
- **ONNX Model Overhead**: Previously, running `mythrax mcp` or CLI commands alongside the daemon would require multiple processes to load the ONNX embedding model into memory, leading to RAM exhaustion. In this client-server model, only the single daemon process loads the model, reducing RAM usage by 50%.
- **Network Latency**: Adding a local HTTP roundtrip (typically <1-2ms) is negligible compared to the database startup overhead of opening RocksDB and loading SurrealDB (typically >100-200ms). The client-server model is actually *faster* for individual command execution.
