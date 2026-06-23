# Tasks

## T1: SurrealDB Schema Update and Storage Backend Methods
-   **Purpose**: Update SurrealDB schema with the new `short_term_memory` table and implement CRUD backend methods.
-   **Related Requirements**: STM Database table requirement.
-   **Related Tests**: STM DB Unit Test.
-   **Inputs**: Database connection.
-   **Actions**:
    1.  Add `short_term_memory` table, fields (`session_id`, `key`, `value`, `updated_at`), and `stm_session_key` unique index to `src/db/schema.rs`.
    2.  Add `save_stm`, `get_stm`, and `clear_stm` declarations to the `StorageBackend` trait in `src/db/backend.rs`.
    3.  Implement these methods in `SurrealBackend` in `src/db/backend.rs` using SurrealDB queries.
-   **Expected Output**: SurrealDB compiles with updated schemas and backend support.
-   **Validation**: `cargo check` and running unit tests.

## T2: Short Term Memory (STM) Implementation
-   **Purpose**: Implement the STM service layer with dual-write to local JSON files under `.handoffs/`.
-   **Related Requirements**: STM JSON File Dual-Write.
-   **Related Tests**: STM File Dual-Write Unit Test.
-   **Inputs**: `session_id`, `key`, `value`.
-   **Actions**:
    1.  Implement a workspace root finder helper.
    2.  Write a helper in `src/store.rs` or `src/lib.rs` to write the JSON to `.handoffs/stm_<session_id>.json`.
    3.  Implement deletion helper to delete `.handoffs/stm_<session_id>.json` from disk when `clear_stm` is executed.
    4.  Ensure values are cleaned using `SecretFilter::clean`.
-   **Expected Output**: STM mutations write to both SurrealDB and `.handoffs/`, and `clear_stm` deletes files from disk.
-   **Validation**: Inspecting the local files after save and clear operations.

## T3: MCP Server STM Tool Routing
-   **Purpose**: Route MCP requests for `put_short_term`, `get_short_term`, and `clear_short_term` to the STM service layer.
-   **Related Requirements**: MCP STM Tools.
-   **Related Tests**: MCP STM Methods test.
-   **Inputs**: JSON-RPC params.
-   **Actions**:
    1.  Expose `put_short_term`, `get_short_term`, and `clear_short_term` in `tools/list` schema in `src/mcp.rs`.
    2.  Wire the calls to the database backend and store file helpers in `src/mcp.rs`'s `call_tool`.
-   **Expected Output**: JSON-RPC handler responds to STM methods.
-   **Validation**: Run MCP integration test.

## T4: Add pdf-extract Dependency and Integration
-   **Purpose**: Add the `pdf-extract` library to Cargo.toml and write helper code to extract text from local PDFs.
-   **Related Requirements**: Pure-Rust PDF parser.
-   **Related Tests**: PDF Parser Unit Test.
-   **Inputs**: Local PDF file path.
-   **Actions**:
    1.  Add `pdf-extract = "0.10.0"` under dependencies in `Cargo.toml`.
    2.  Implement a helper function to read PDF files and return extracted text.
-   **Expected Output**: Text extracted from PDF files successfully in Rust code.
-   **Validation**: Add a unit test to verify PDF text extraction.

## T5: Ingestion Forge Pipeline
-   **Purpose**: Implement the core `Forge` module in Rust to chunk text, call the LLM, and parse extracted rules/insights.
-   **Related Requirements**: Forge Ingestion pipeline.
-   **Related Tests**: Forge Ingestion Pipeline Integration Test.
-   **Inputs**: Source file path, scope.
-   **Actions**:
    1.  Create `src/cognitive/forge.rs` containing `Forge` struct and chunking logic.
    2.  Add logic to parse PDF files using the `pdf-extract` helper if the file extension is `.pdf`.
    3.  In a loop, query the LLM to extract structured capability rules and insights.
    4.  Save rules to `wisdom/forge/` and insights to `wiki/forge/` using `MarkdownStore::write_file`.
    5.  Call `save_wisdom_rule` and `save_wiki_node` to update SurrealDB with embeddings.
-   **Expected Output**: Text processed and capability wisdom/insights stored.
-   **Validation**: Automated integration test in `tests/test_forge.rs`.

## T6: MCP Server and CLI Ingestion Integration
-   **Purpose**: Add `forge` subcommand and `forge_source` MCP tool.
-   **Related Requirements**: Forge Ingestion interface.
-   **Related Tests**: Integration test.
-   **Inputs**: File path and scope.
-   **Actions**:
    1.  Expose `forge_source` in the MCP tools list and handler in `src/mcp.rs`.
    2.  Add the `forge` subcommand to `src/cli.rs`.
-   **Expected Output**: Forge is triggerable via both MCP and CLI.
-   **Validation**: Run `mythrax-core forge test.txt`.

## T7: Handoff Skill Consolidation and Deprecation
-   **Purpose**: Merge global handoff rules into local `mythrax` skill, deprecate the global handoff skill, and mandate AST links in handoffs.
-   **Related Requirements**: Skill Consolidation, AST-Driven Handoffs.
-   **Related Tests**: Handoff AST Parser Test.
-   **Inputs**: Global and local skill files.
-   **Actions**:
    1.  Consolidate `agent-handoff/SKILL.md` rules into `.agents/skills/mythrax/SKILL.md`.
    2.  Add deprecation banner to `/Users/keith/.gemini/config/skills/agent-handoff/SKILL.md`.
    3.  Update local templates and documentation to enforce AST symbolic references and line-anchored links.
    4.  Update the unified `mythrax` skill to explicitly state that follow-up fetching (pagination) is only required for memories about skills or wisdom matches, and that other memory retrieval is optional but strongly recommended.
    5.  Detail memory conflict resolution precedence hierarchy in the `mythrax` skill so that agents handle contradiction deterministically and mandate surfacing all conflicts/ambiguities (with decisions) in the implementation plan.
-   **Expected Output**: Skills consolidated and original deprecated.
-   **Validation**: Verify skills are readable and deprecated correctly.

## T8: CLI E2E Tests
-   **Purpose**: Add automated E2E tests for STM and Forge CLI commands.
-   **Related Requirements**: 100% Automated verification.
-   **Related Tests**: CLI STM Lifecycle E2E, CLI Forge Ingestion E2E.
-   **Inputs**: Compiled binary, mock files.
-   **Actions**:
    1.  Create `tests/test_cli_e2e.rs`.
    2.  Write tests that execute the `mythrax-core` binary via `std::process::Command` for both `stm` and `forge` commands, verifying exit codes, standard streams, and workspace side-effects (JSON files, markdown files).
-   **Expected Output**: Automated E2E tests executing and passing as part of `cargo test`.
-   **Validation**: Run `cargo test --test test_cli_e2e`.

## T9: Scheduled Handoff Cleanup Task
-   **Purpose**: Add scheduled background task in daemon loop to clean up completed/failed handoffs.
-   **Related Requirements**: Scheduled Handoff File Cleanup.
-   **Related Tests**: Stale Handoff Background Cleanup Test.
-   **Inputs**: Database connection, background scheduler daily loop.
-   **Actions**:
    1.  Add `delete_stale_handoffs` method to the backend `StorageBackend` trait and implement it in `SurrealBackend` to find and delete completed/failed handoffs older than 7 days.
    2.  Integrate the cleanup call inside the background scheduler daily loop in `src/main.rs`.
    3.  In the cleanup logic, also delete the corresponding handoff contract and STM JSON files from the workspace `.handoffs/` directory.
-   **Expected Output**: Stale completed/failed handoff contracts automatically deleted daily by daemon.
-   **Validation**: Unit and integration tests in `test_stm.rs`.

## T10: Lean Skill Refactoring
-   **Purpose**: Refactor skills in the workspace to follow the Lean Skill Paradigm.
-   **Related Requirements**: Refactor local `mythrax` skill to follow the Lean Skill Paradigm.
-   **Related Tests**: None (documentation / playbook content refactor).
-   **Inputs**: Workspace skill files.
-   **Actions**:
    1.  Review and edit `.agents/skills/mythrax/SKILL.md` to skeletonize instructions (reducing verbosity).
    2.  Move heavy examples, playbooks, or reference guides into `references/` or `examples/` subfolders.
    3.  Verify that the condensed playbooks reference the memory engine and STM for active execution context.
-   **Expected Output**: Workspace skills are highly condensed and optimized for token budget.
-   **Validation**: Check token count of modified skills.
