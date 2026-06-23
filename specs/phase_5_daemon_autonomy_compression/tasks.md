# Tasks: Phase 5 (v0.6.0) — Daemon Autonomy & Node Compression

This task list breaks down the implementation of Phase 5 (v0.6.0) into discrete, testable development steps.

---

## T1: Implement Continuous Pruning Core
- **Purpose**: Add the database-level and disk-level stale memory/handoff pruning.
- **Related Requirements**: Section 1 (Continuous Pruning)
- **Related Tests**: U1 (Stale Memory Pruning), U2 (Fresh Protection)
- **Inputs**: Vault root path, SurrealDB client.
- **Actions**:
  1. Add `async fn prune_stale_memories(&self, vault_root: &std::path::Path) -> Result<()>;` to the `StorageBackend` trait in `db/backend.rs`.
  2. Implement `prune_stale_memories` in `SurrealBackend`:
     - Run `DELETE FROM short_term_memory WHERE updated_at < time::now() - 3d;`.
     - Update `delete_stale_handoffs()` to use `3d` threshold instead of `7d`.
     - Scan the `.handoffs/` folder in the vault, check the modification time of `stm_*.json` files, and delete any files older than 3 days.
- **Expected Output**: Expired database records and disk files are deleted.
- **Validation**: Verify compilation.

---

## T2: Integrate Continuous Pruning into background loops
- **Purpose**: Ensure pruning runs automatically at all phase checkpoints.
- **Related Requirements**: Section 1 (Continuous Pruning)
- **Related Tests**: I1 (Compactor and Dreaming integration)
- **Inputs**: `DreamCoordinator`, `Compactor`.
- **Actions**:
  1. Modify `DreamCoordinator::run_dream` in `synthesis.rs` to call `backend.prune_stale_memories(vault_root)` at the end of the dream process.
  2. Modify `Compactor::compact_scope` and `Compactor::compact_global` in `compactor.rs` to call `backend.prune_stale_memories(vault_root)` at the start of compaction.
- **Expected Output**: Background dreaming and compaction cycles run pruning.
- **Validation**: Verify compactor and dreaming test suites pass.

---

## T3: Implement Fine-Grained Inner-Node Compaction
- **Purpose**: Dynamically compress nodes to fit token budget rather than omitting them.
- **Related Requirements**: Section 2 (Inner-Node Compaction)
- **Related Tests**: U3 (Wisdom Compaction), U4 (Episode Compaction)
- **Inputs**: Token budget, Search candidates.
- **Actions**:
  1. Implement `compact_search_result(&self, item: &mut SearchResult, remaining_budget: usize) -> bool` helper in `SurrealBackend` (`db/backend.rs`).
  2. If the node is a wisdom rule (contains `**Why**:`), strip the causal explanation.
  3. If it is an episode or insight, extract the first paragraph (up to the first `\n\n`), or binary-search character truncation length to fit the budget exactly. Ensure suffix `\n... [Truncated (Inner-Node Compaction)]` is only added if content was actually shortened.
  4. Update `SurrealBackend::search` budget loop to attempt `compact_search_result` on candidates exceeding remaining budget before omitting.
- **Expected Output**: Real-time searches return compressed results when budget is constrained.
- **Validation**: Check search functionality works with custom budget tests.

---

## T4: Implement Episode Filtering & Exclusions
- **Purpose**: Exclude raw episodes by default to prevent context bloat.
- **Related Requirements**: Section 4 (Episode Filtering)
- **Related Tests**: U5 (Default Search Excludes Episodes), U6 (Traversal Excludes Episodes), A2 (CLI Search Episodes Flag)
- **Inputs**: `include_episodes` parameter.
- **Actions**:
  1. Update signature of `StorageBackend::search` in `db/backend.rs` to accept `include_episodes: bool`.
  2. In `SurrealBackend::search`:
     - If `include_episodes` is false, omit `episode` query and remove `episode` from relates_to/mentions target tables. Make sure index mapping parses `wiki_node` and `wisdom` correctly (since indices shift without `episode`).
     - If `include_episodes` is true, check `allow_downward` to determine traversal direction `<->` or `->`.
  3. Add `include_episodes` flag to MCP `search_memories` tool in `mcp.rs`.
  4. Add `include_episodes` to Axum route `/v1/search` in `api.rs`.
  5. Add `episodes: bool` flag to `Commands::Search` in `cli.rs` and update the payload sent in `main.rs`.
  6. Update all test cases calling `.search(...)` in `db/backend.rs` and `vault/watcher.rs` to pass `true` for `include_episodes` where they verify episodes.
- **Expected Output**: Search excludes episodes by default but includes them if explicitly requested.
- **Validation**: Verify all tests pass.

---

## T5: Add Daemon CLI Run Subcommand
- **Purpose**: Enable foreground running of the daemon with PID tracking and clean shutdown.
- **Related Requirements**: Section 3 (Daemon CLI)
- **Related Tests**: A1 (Daemon CLI Run)
- **Inputs**: Clap CLI, Axum server.
- **Actions**:
  1. Update `DaemonAction` in `cli.rs` to include `Run { port: u16, vault: Option<String> }`.
  2. In `main.rs`, handle `DaemonAction::Run` by starting the Axum HTTP REST server and background scheduler loops in the foreground.
  3. Catch Ctrl+C / SIGINT signals using `tokio::signal::ctrl_c()`, delete the PID file upon exit, and shut down cleanly.
- **Expected Output**: A foreground terminal runner command `mythrax daemon run` that manages its PID.
- **Validation**: `cargo run -- daemon run --help` runs successfully.

---

## T6: Implement Verification Tests
- **Purpose**: Verify the correctness of all implemented tasks.
- **Related Requirements**: All
- **Related Tests**: U1-U6, I1, A1-A2
- **Inputs**: Test framework.
- **Actions**:
  1. Add tests U1 and U2 to `mythrax-core/tests/test_stm.rs`.
  2. Add tests U3 and U4 to `mythrax-core/tests/test_compactor.rs`.
  3. Add tests U5 and U6 to `mythrax-core/src/db/backend.rs`.
  4. Add tests A1 and A2 to `mythrax-core/tests/test_cli_e2e.rs`.
- **Expected Output**: 100% tests compiled and passing.
- **Validation**: Execute `cargo test`.
