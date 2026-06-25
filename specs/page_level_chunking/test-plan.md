# Test Plan: Page-Level Chunking & Graph Linking

## Unit Tests
Modify/create unit tests in `mythrax-core/src/vault/ingestion.rs` inside the `tests` module to verify the correctness of the hierarchical, recursive splitting and grouping logic:
1. **`test_chunk_parsed_content_simple`**: A small string under 20,000 characters must return a single chunk matching the input.
2. **`test_chunk_parsed_content_paragraph_boundary`**: A string with multiple paragraphs that exceeds the limit must split cleanly at paragraph boundaries (`\n\n`) and group them up to the limit without splitting inside a paragraph.
3. **`test_chunk_parsed_content_line_fallback`**: A single paragraph exceeding the limit must split cleanly at line boundaries (`\n`).
4. **`test_chunk_parsed_content_character_fallback`**: A single line exceeding the limit (with no spaces) must split by character.
5. **`test_chunk_parsed_content_page_boundary`**: Verify that frontmatter headers (enclosed in the first `---` block) are preserved in the first chunk, and subsequent `---` lines are treated as hard page boundaries where no grouping occurs.

---

## Integration Tests
Update the following integration test files to align with the new 20,000 character limit:

1. **`mythrax-core/tests/test_semantic_document_splitting_relations.rs`**:
   - Change the large paragraph size to exceed 20,000 characters.
   - Assert that chunks are created at the 20,000-character boundary.
   - Assert that the collapsible navigation callout (`> [!INFO] Navigation`) is correctly written to each chunk file on disk.
   - Assert that the parent document has the `## Chunks` index written at the bottom.
   - Assert that `relates_to` edges (`next`/`prev`/`parent`) are correctly established in SurrealDB.

2. **`mythrax-core/tests/test_forge.rs`**:
   - In `test_second_pass_character_chunking`, change the text size from `100_000` to `20_000`.
   - Assert that the split sections have a maximum length of `20,000` characters.

3. **`mythrax-core/tests/test_vault_lifecycle.rs`**:
   - In `test_large_artifact`, change the generated artifact size to ~25,000 characters.
   - Assert that `bulk_ingest_vault` splits the artifact into 2 parts under the new `20,000` character limit.
   - Assert that the physical parent index file and parent episode node are correctly created and related in SurrealDB.

---

## Edge Cases
- **Empty File**: Verify that an empty text input returns a single empty chunk or an empty vector without panicking.
- **No Available Clean Boundaries**: Verify that a document with no page breaks, no sections, and no paragraphs (just a single massive block of text) falls back gracefully to line/word/character splitting.
- **Mock Database Backend**: Verify that when using a mock/test database backend (which does not support raw SurrealDB operations), the downcasting in `bulk_ingest_vault` fails gracefully and continues without panicking.

---

## Failure Modes
- **Lock Contention**: Ensure that re-running ingestion does not trigger database file locks, which is achieved by cleanly stopping the daemon before reset.
- **Quarantine Handling**: Verify that if a transcript is corrupted, it is quarantined successfully and recorded as an error instead of halting the entire bulk ingestion run.
