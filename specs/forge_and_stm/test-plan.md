# Test Plan

## Unit Tests
-   [ ] **STM DB Unit Test**: Test `save_stm`, `get_stm`, and `clear_stm` against an in-memory SurrealDB instance.
-   [ ] **STM File Dual-Write Unit Test**: Verify that saving to STM writes correctly to `.handoffs/stm_<session_id>.json` and redacts secrets.
-   [ ] **PDF Parser Unit Test**: Test PDF parsing with a mock PDF to ensure correct text extraction using the `pdf-extract` crate.
-   [ ] **Text Chunking Unit Test**: Verify `cognitive::forge` splits text into sized blocks with expected overlap.
-   [ ] **STM File Deletion Unit Test**: Verify that calling `clear_stm` successfully deletes `.handoffs/stm_<session_id>.json` from disk.

## Integration Tests
-   [ ] **Forge Ingestion Pipeline**: Ingest a mock text document and verify that the extracted Wisdom Rules and Wiki Nodes are successfully saved to both the filesystem (under `wisdom/forge/` and `wiki/forge/`) and SurrealDB (indexed with HNSW embeddings).
-   [ ] **MCP STM Methods**: Exercise `put_short_term`, `get_short_term`, and `clear_short_term` JSON-RPC calls over the MCP interface.
-   [ ] **Handoff AST Parser Test**: Verify that handoff contract parser validates target context bodies containing exact AST class/function symbols and line-anchored links.
-   [ ] **Stale Handoff Background Cleanup Test**: Verify that the daily cleanup routine detects completed/failed handoffs older than 7 days, and successfully deletes their filesystem contract/STM files and SurrealDB database records.

## E2E Tests
-   [ ] **CLI STM Lifecycle E2E**: Execute `mythrax-core stm` CLI put/get/clear subcommands using mock session IDs, verifying output stdout, database state, and disk file sync/cleanup.
-   [ ] **CLI Forge Ingestion E2E**: Execute `mythrax-core forge <path>` command, verify that the compiled binary chunkifies the text, calls the LLM, writes files to Obsidian directories, generates embeddings, and makes the newly forged capabilities searchable via `search_wisdom` vector query.

## Acceptance Tests
-   [ ] **Enhancement of Agent Handoff**: A parent agent can save variables to STM, spawn a subagent, and the subagent can load those variables to successfully complete a task.
-   [ ] **Forge capability indexing**: Forge a document containing capability guidelines, query the database using standard vector search (`search_wisdom`), and confirm that the forged rules/hints are returned.

## Edge Cases
-   [ ] **STM Empty Session**: Querying `get_stm` for a non-existent session must return an empty JSON object, not an error.
-   [ ] **Secrets Leak in STM**: Writing a value like `key="api_key", value="1234-abcd-xyz"` must sanitize the value using `SecretFilter` before writing to file.
-   [ ] **Duplicate Forge Runs**: Re-forging the same document must update existing nodes rather than creating duplicates.

## Failure Modes
-   [ ] **SurrealDB Offline**: If SurrealDB is offline, the STM system must gracefully fallback to the local file storage and issue warnings.
-   [ ] **Malformed PDF Ingestion**: If a PDF is corrupted, the Forge pipeline must log the error and terminate gracefully without panic.
