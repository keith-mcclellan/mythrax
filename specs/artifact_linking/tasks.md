# Tasks: Artifact Ingestion Linking & Local Model Stabilization

## T1: Implement Ingest Log-Chunking & Content Formatting
- **Purpose**: Implement a log-chunking utility, pre-scan artifacts in `bulk_ingest_vault` for the `antigravity` harness, format links, and write bidirectional wikilinks to both the episode part files and the artifact files.
- **Related Requirements**: Requirement 1 & 2
- **Inputs**: `src/vault/ingestion.rs`
- **Actions**:
  - Implement a `chunk_parsed_content` helper to split parsed logs into chunks of max 100,000 characters.
  - Scan the directory `path` for all `.md` files to collect their names.
  - For each chunk, write it as a separate part note with `## Linked Artifacts` links to all artifacts.
  - Append the `Source Episodes` backlink list pointing to all parts in each artifact file.
- **Expected Output**: Linked and chunked Obsidian files generated during ingestion.

## T2: Database Relationship Query & Setup
- **Purpose**: Add `get_related_node_ids` to `StorageBackend` trait, implement in `SurrealBackend`, and call `relate_nodes` for all parts during ingestion.
- **Related Requirements**: Requirement 3
- **Inputs**: `src/db/backend.rs`, `src/vault/ingestion.rs`
- **Actions**:
  - Define `async fn get_related_node_ids(&self, from_id: &str) -> Result<Vec<String>>` in `StorageBackend` trait.
  - Implement in `SurrealBackend` using:
    `SELECT VALUE out FROM relates_to WHERE in = $from;`
  - In `bulk_ingest_vault` for `antigravity` case, relate each saved episode part to each saved artifact.
- **Expected Output**: Graph relationships queryable via the new trait method.

## T3: Enriched LLM Summary Prompt
- **Purpose**: Query related artifacts for each episode part during `run_dream` and include their contents in the LLM prompt. Apply safety window truncation at 100,000 characters.
- **Related Requirements**: Requirement 4
- **Inputs**: `src/cognitive/synthesis.rs`
- **Actions**:
  - In both incremental merge and cluster analysis loops, fetch related node IDs using `db.get_related_node_ids` for the episode part.
  - Fetch node contents using `db.get_memory_nodes`.
  - Format and append wiki node contents to the episode part content.
  - Truncate final combined context to 100,000 characters.
- **Expected Output**: Prompts contain the text of associated artifacts and are bounded safely.

## T4: Local LLM Client Stabilization
- **Purpose**: Reduce KV cache memory footprint, improve HTTP connection recovery/resilience, and implement cache recovery pause.
- **Related Requirements**: Requirement 5, 6, & 7
- **Inputs**: `src/llm/mod.rs`
- **Actions**:
  - Set `max_tokens` default to 8192 in local completion payload.
  - Implement a 5-second sleep (`tokio::time::sleep`) after each local completion request.
  - Truncate prompt to 100,000 characters if it exceeds limit.
  - Update `send_with_retry` to execute 6 attempts with max 5s backoff delay.
  - Print user warning if local connection fails.
- **Expected Output**: Stable, self-healing HTTP request loop to local LLM with 8k output ceiling and 5s cooldown pause.

## T5: Verify and Add Tests
- **Purpose**: Write tests verifying log chunking, file links, database edges, and artifact prompt enrichment.
- **Related Requirements**: Requirement 5
- **Inputs**: `tests/test_vault_lifecycle.rs` or unit tests
- **Actions**:
  - Setup a mock ingestion test with a transcript exceeding 100,000 characters.
  - Assertions verifying that:
    1. The log is split into multiple part files.
    2. All part files link to the artifacts.
    3. The artifacts link to all parts in their footers.
    4. SurrealDB relates each part to each artifact.
  - Assertions in dreaming tests verifying artifact contents are pulled in.
  - Run `cargo test`.



