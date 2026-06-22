# Requirements - Mythrax Project Reinitialization

## Problem
The Mythrax local memory system needs a complete reset and reinitialization of its data stores (Obsidian vault, database cache, and vector embeddings) to clear old v0.10 metadata, rebuild the knowledge graph, and re-index all historical agent transcripts using the new Rust core.

Additionally, the Rust core daemon and MCP server currently run in transient in-memory SurrealDB sessions and lack support for the full set of v0.10 lifecycle tools (bulk ingestion, vault organization, wiki summarization, and self-healing vault integrity verification). To perform a complete, clean reinitialization and remain feature-complete with v0.10, the Rust core must support persistent database sessions and expose these operations.

Finally, the initialization and configuration system must support new users setting up clean environments, as well as users running multiple agent harnesses (e.g. Antigravity, Claude Code, Cursor, Codex, OpenCode, OpenClaw, Hermes) concurrently on their machine, without hardcoding user-specific paths or repeating full memory resets. This must include automatically resolving and ingesting pre-existing history from the chosen harness so that users do not have to perform manual seeding or verification.

## Outcome
A clean, fully re-ingested Mythrax vault and SurrealDB database populated with up-to-date vector embeddings generated locally via the Nomis ONNX embedder. All legacy and auto-generated vault files are quarantined or reset, and the Rust CLI and MCP server natively support persistent RocksDB configuration and lifecycle tools.

The `mythrax init` and `mythrax config` commands support automatic harness-specific history location and bulk ingestion out-of-the-box for all 7 supported harnesses.

## User Value
- Consolidates all history into a clean, well-organized Obsidian vault.
- Enables persistent, fast local memory retrieval via RocksDB database cache.
- Restores all native MCP tools (`bulk_ingest`, `organize_vault`, `summarize_episodes`, `verify_vault_integrity`) for AI agents using the Rust core.
- Allows seamless switching and setup of multiple developer interfaces (harnesses).
- Eliminates any remaining Python dependencies for memory operations.
- Automates history seeding on first setup or harness addition.

## In Scope
1. **Persistent RocksDB Storage**: Update `SurrealBackend` to support persistent RocksDB sessions by parsing the `surrealdb_url` from `~/.mythrax/config.json`.
2. **Embeddings Integration**: Integrate `LocalEmbedder` into `SurrealBackend::save_episode` to automatically compute and write 768-dimensional Nomis embeddings on save.
3. **Lifecycle Tools in Rust**: Implement bulk-ingestion, vault organization, wiki summarization, and self-healing vault integrity checks in `mythrax-core` (CLI subcommands and MCP tools). Implement a background scheduler loop inside the daemon to automatically run multi-level dreaming (incremental debounced, incremental threshold (>50 episodes), and daily deep) followed by hierarchical compaction.
4. **Bootstrapping Init**: Update `mythrax init [harness] [--source <path>]` to set up clean RocksDB databases, SurrealDB schemas, Obsidian Vault directories, and configure the target harness (inject skills, MCP config, and hooks).
5. **Dynamic Harness Configuration**: Implement `mythrax config <harness> [--source <path>]` to dynamically write MCP and hook configuration files for the specified client (e.g. `antigravity`, `claude`, `cursor`, `codex`, `opencode`, `openclaw`, `hermes`) without wiping the database. Expose `mythrax config llm` and update `/v1/config/llm` to allow users and harness agents to dynamically get/set LLM/embedding providers, models, and cloud API keys securely.
6. **Automatic History Discovery**: Implement auto-resolution of default log directories for all supported harnesses:
   - `antigravity` -> `~/.gemini/antigravity/brain/`
   - `claude` -> `~/.claude/projects/`
   - `cursor` -> Cursor global state database (macOS/Linux/Windows path)
   - `codex` -> `~/.codex/logs/` or `~/.codex/history/`
   - `opencode` -> `~/.opencode/sessions/`
   - `openclaw` -> `~/.openclaw/history/`
   - `hermes` -> `~/.hermes/state.db`
7. **Immediate History Ingestion**: Trigger the bulk-ingestion sequence automatically during `init` and `config` if a history source path is found or resolved.
8. **Reinitialization Execution**:
   - Clean/wipe the RocksDB directory.
   - Clean/wipe auto-generated folders in the Obsidian vault (preserving manual assets/configs).
   - Re-run ingestion over the historical logs to regenerate all database records and vector embeddings.
   - Re-run wiki summarization to rebuild the knowledge graph.

## Out of Scope
- Migrating/converting external databases other than local agent transcripts.
- Modifying manual files or configurations inside the user's `.obsidian` directory.

## Inputs
- Configuration: `~/.mythrax/config.json` containing the vault root and SurrealDB URL.
- Models: Nomis ONNX model files in `~/.mythrax/models/`.
- Ingestion Source: Antigravity brain transcripts folder `/Users/keith/.gemini/antigravity/brain/`.

## Outputs
- Reinitialized RocksDB database in `~/.mythrax/db/`.
- Reorganized Obsidian vault in `/Users/keith/mythrax-vault/`.
- Active daemon and MCP server pointing to the persistent database.
- Harness config files (`hooks.json`, `mcp_config.json`, `~/.claude.json`, `~/.cursor/mcp.json`, `~/.codex/config.toml`, `~/.opencode/config.json`, `~/.openclaw/config.json`, `~/.hermes/config.json`).

## Constraints
- **Zero Deletions on Vault Reset**: Do not use `rm -rf` directly on vault folders containing user files; always move deprecated folders to `/Users/keith/mythrax-vault/.trash/` or quarantine them.
- **Local Embedding Execution**: Embeddings must be generated using the local ONNX runtime to avoid cloud latency and billing.
- **Fail-Safe Tests**: The database must fall back gracefully to non-embedded operation in environments where ONNX models are missing (e.g. standard cargo test runner).
- **No Hardcoded Executable Paths**: All MCP and hook configurations must retrieve the current executable path dynamically via `std::env::current_exe()`.

## Acceptance Criteria
- [ ] SurrealDB supports persistent RocksDB sessions based on the configuration file URL.
- [ ] `save_episode` calculates Nomis vector embeddings using the local ONNX model and saves them to the database.
- [ ] CLI and MCP server support `bulk_ingest`, `organize_vault`, `summarize_episodes`, and `verify_vault_integrity`.
- [ ] All 44 existing cargo tests pass cleanly.
- [ ] `mythrax init` sets up fresh RocksDB databases, vault structures, and registers the chosen harness.
- [ ] `mythrax config <harness>` correctly writes configuration files for all 7 supported harnesses using the dynamically resolved binary path.
- [ ] Harness configurations auto-discover and bulk-ingest their respective historical transcripts into the database.
- [ ] Running the reinitialization procedure successfully clears the database and vault, re-ingests the history, and generates non-empty embeddings for all episodes.
