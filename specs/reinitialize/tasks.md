# Tasks - Mythrax Project Reinitialization & Harness Configuration

## T1: Enable Persistent RocksDB Storage
- **Purpose**: Allow the Rust daemon and CLI to read the configured `surrealdb_url` and connect to a persistent RocksDB session instead of a transient in-memory session.
- **Inputs**: `src/db/backend.rs`, `src/main.rs`, `src/mcp.rs`.
- **Actions**:
  1. Modify `SurrealBackend` struct to support dynamic local engines.
  2. Implement `SurrealBackend::new(url: &str)` to connect via `RocksDb` or `Mem`.
  3. Update `main.rs` and `mcp.rs` to load the URL from the configuration file instead of hardcoding `new_in_memory()`.
- **Expected Output**: Daemon and CLI can connect to and read/write a persistent RocksDB cache on disk.
- **Validation**: Verify that running the daemon writes to `~/.mythrax/db` and that data persists across daemon restarts.

## T2: Integrate LocalEmbedder into SurrealBackend
- **Purpose**: Automatically compute 768-dimensional Nomis text embeddings when episodes are saved to the database.
- **Inputs**: `src/db/backend.rs`, `src/embeddings.rs`.
- **Actions**:
  1. Update `SurrealBackend` struct to include `Option<Arc<LocalEmbedder>>`.
  2. Attempt to construct `LocalEmbedder` on backend creation. If model files are missing, log a warning and fallback to `None`.
  3. In `save_episode`, if `embedder` is present, call `embed` on the episode's plain text content.
  4. Write the vector to the `embedding` field in the SurrealDB `UPDATE` and `CREATE` statements.
- **Expected Output**: Saved episodes have non-empty embeddings.
- **Validation**: Run existing cargo tests (specifically `test_surreal_db_operations`). Confirm that all tests pass.

## T3: Implement Clean Init and Harness Configuration Subcommands
- **Purpose**: Provide CLI commands to initialize clean database/vault setups and dynamically configure client harnesses with immediate history ingestion for all 7 harnesses.
- **Inputs**: `src/cli.rs`, `src/main.rs`, `src/vault/ingestion.rs`.
- **Actions**:
  1. Add `Init` (with optional `harness` and `source` parameters) and `Config` (with required `harness` and optional `source` parameters) commands.
  2. In `Commands::Init`, reset the RocksDB directory and generate default vault subfolders.
  3. Implement `config_harness(name: &str, source: Option<&str>)` to resolve `std::env::current_exe()`.
  4. Auto-locate default history paths if `source` is `None` (for all 7 harnesses).
  5. Run `bulk_ingest_vault` automatically on configuration if a path is resolved.
  6. Add `"codex"` parser in `src/vault/ingestion.rs` to handle Codex transcripts.
  7. Implement specific config writers for all 7 harnesses.
- **Expected Output**: CLI subcommands are functional, dynamically configure user files, and automatically ingest history.
- **Validation**: Run `mythrax init antigravity` and `mythrax config claude` and inspect output config files and vault entries.

## T4: Implement Lifecycle CLI/MCP Commands in Rust
- **Purpose**: Provide CLI and MCP interfaces to manage vault operations natively in Rust.
- **Inputs**: `src/cli.rs`, `src/main.rs`, `src/mcp.rs`.
- **Actions**:
  1. Add `vault` subcommand group in `src/cli.rs` (`ingest`, `organize`, `summarize`, `verify`, `reprocess`).
  2. Implement their respective behaviors in `src/main.rs` and `src/mcp.rs`.
- **Expected Output**: Native Rust-powered memory operations are fully wired up.
- **Validation**: Verify JSON-RPC outputs and help menus.

## T5: Execute Reset and Reinitialization
- **Purpose**: Run the actual reinitialization steps on the user's environment.
- **Inputs**: CLI scripts, brain history logs, models.
- **Actions**:
  1. Stop the running daemon.
  2. Archive `~/mythrax-vault/` episodes/wiki folders to `~/mythrax-vault/.trash/`.
  3. Run `mythrax init antigravity` to set up clean RocksDB and vault structures and auto-ingest history.
  4. Run `mythrax vault summarize` and verify.
- **Expected Output**: Vault is clean and database is populated with clean records and vector embeddings.
- **Validation**: Query the database using a test search or direct command to verify embeddings are present.
