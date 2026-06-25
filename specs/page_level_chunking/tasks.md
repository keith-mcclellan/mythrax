# Implementation Tasks: Page-Level Chunking & Graph Linking

## T1: Hierarchical Recursive Chunker in `ingestion.rs`
- **Purpose**: Replace the old line-based chunker with a high-fidelity recursive boundary-honoring chunker.
- **Related Requirements**: A1, A2
- **Related Tests**: `test_chunk_parsed_content_simple`, `test_chunk_parsed_content_paragraph_boundary`, `test_chunk_parsed_content_line_fallback`, `test_chunk_parsed_content_character_fallback`, `test_chunk_parsed_content_page_boundary`
- **Inputs**: `content: &str`, `limit: usize`
- **Actions**:
  - In `mythrax-core/src/vault/ingestion.rs`, implement:
    ```rust
    pub fn chunk_parsed_content(content: &str, limit: usize) -> Vec<String> {
        let pages = split_by_page_breaks(content);
        let mut all_chunks = Vec::new();
        for page in pages {
            let segments = split_recursive(&page, 1, limit);
            let chunks = group_segments(&segments, limit);
            all_chunks.extend(chunks);
        }
        all_chunks
    }
    ```
  - Implement helper function `split_by_page_breaks(text: &str) -> Vec<String>` to split by `\f` or horizontal rules `---`/`***`/`___` on their own line. Ensure it is frontmatter-aware by skipping the first two `---` markers that enclose the YAML frontmatter block at the very beginning of the string.
  - Implement `split_by_sections(text: &str) -> Vec<String>` to split before lines starting with `# `.
  - Implement `split_by_paragraphs(text: &str) -> Vec<String>` to split by `\n\n`.
  - Implement `split_by_lines(text: &str) -> Vec<String>` to split by `\n`.
  - Implement `split_by_words(text: &str, max_chars: usize) -> Vec<String>` to split by whitespace. If a word is longer than `max_chars`, split it by characters.
  - Implement `split_recursive(text: &str, level: usize, max_chars: usize) -> Vec<String>` to apply splitting levels 1 to 4 sequentially.
  - Implement `group_segments(segments: &[String], max_chars: usize) -> Vec<String>` to greedily merge adjacent segments up to the character limit, joining them with double newlines.
- **Expected Output**: A list of chunk strings cleanly split at boundaries, none exceeding the limit.
- **Validation**: Compile `mythrax-core` and run unit tests.

---

## T2: Refactor Episode Ingestion & Graph Linking in `ingestion.rs`
- **Purpose**: Update `bulk_ingest_vault`'s `antigravity` harness to use the new `20,000` limit, generate shared UUIDs, insert collapsible navigation callouts, and relate parts sequentialy and hierarchically in SurrealDB.
- **Related Requirements**: A1, A3, A4, A5, A6
- **Related Tests**: `test_vault_lifecycle.rs` (`test_large_artifact` section)
- **Inputs**: `bulk_ingest_vault` context
- **Actions**:
  - In `mythrax-core/src/vault/ingestion.rs`, inside `bulk_ingest_vault` for the `antigravity` harness:
    - Change the chunking limits from `100_000` to `20_000` for both raw artifacts and transcript parsed content.
    - Generate a single `shared_uuid` using `&uuid::Uuid::new_v4().to_string()[..8]` to represent the entire transcript folder.
    - Save all parts using filenames: `episodes/antigravity_{dir_name}_part{chunk_idx+1}_{shared_uuid}.md` (or `episodes/antigravity_{dir_name}_{shared_uuid}.md` if only 1 part).
    - If `total_chunks > 1`, write a physical parent index file to `episodes/antigravity_{dir_name}_{shared_uuid}.md` containing links to all parts, and save a parent episode record in SurrealDB.
    - For each part, append a collapsible `> [!INFO] Navigation` callout at the bottom containing links to the parent, prev, and next parts.
    - Downcast the database instance to `SurrealBackend` using `db.as_any().downcast_ref::<SurrealBackend>()`.
    - If downcasting succeeds, run SurrealDB `RELATE` queries to link each part to the parent episode record (relation: `'parent'`), and link adjacent parts bidirectionally (relation: `'next'` and `'prev'`).
- **Expected Output**: All parts saved with the same UUID suffix, linked sequentially and hierarchically in Obsidian and SurrealDB.
- **Validation**: Check that compilation succeeds.

---

## T3: Refactor Document Forging in `forge.rs`
- **Purpose**: Update Forge semantic chunking, callout navigation, parent document index, and TOC splitting to use the new `20,000` character limit.
- **Related Requirements**: A1, A3, A4, A5
- **Related Tests**: `test_semantic_document_splitting_relations.rs`
- **Inputs**: `Forge` context in `forge.rs`
- **Actions**:
  - In `mythrax-core/src/cognitive/forge.rs`:
    - Rewrite `semantic_chunk_text` to call `crate::vault::ingestion::chunk_parsed_content(content, 20_000)`.
    - In `ingest_document`, append the collapsible `> [!INFO] Navigation` callout at the bottom of each chunk's markdown file, pointing to the parent, prev, and next chunks.
    - In `ingest_document`, append a Chunks index (`## Chunks`) at the bottom of the parent index markdown file.
    - In `split_into_logical_sections`, change the character limit check and chunking call from `100_000` to `20_000` characters.
- **Expected Output**: Forge chunks split at 20,000 characters with complete collapsible navigation callouts and parent index links.
- **Validation**: Check that compilation succeeds.

---

## T4: Update Integration Test Suite
- **Purpose**: Update integration tests to reflect the new `20,000` character chunk limit.
- **Related Requirements**: A8
- **Related Tests**: `test_semantic_document_splitting_relations.rs`, `test_forge.rs`, `test_vault_lifecycle.rs`
- **Inputs**: Test suite files
- **Actions**:
  - In `mythrax-core/tests/test_semantic_document_splitting_relations.rs`:
    - Update the large paragraph generator to exceed `20,000` characters (e.g. repeat "word " 5,000 times to create a 25,000 character paragraph).
    - Update assertions to verify that chunks are correctly split at the new 20,000 character limit.
  - In `mythrax-core/tests/test_forge.rs`:
    - In `test_second_pass_character_chunking`, update the text size and character limit assertions from `100_000` to `20_000`.
  - In `mythrax-core/tests/test_vault_lifecycle.rs`:
    - In `test_large_artifact`, change the generated artifact size to ~25,000 characters and assert that it chunks into 2 parts under the new `20,000` limit.
- **Expected Output**: The entire test suite compiles and all tests pass successfully.
- **Validation**: Run `cargo test --manifest-path mythrax-core/Cargo.toml` and verify that all 68+ tests pass.

---

## T5: Scope Resolution & Phantom Project Prevention
- **Purpose**: Prevent hidden/metadata folders and generic path names (like `git` or `ref`) from being resolved as project scopes or scanned by the ingestion pipeline.
- **Related Requirements**: A10
- **Related Tests**: None (Manual verification)
- **Inputs**: `resolve_scope_from_path` and `bulk_ingest_vault` in `ingestion.rs`
- **Actions**:
  - In `resolve_scope_from_path`:
    - Skip any path component that starts with `.` (e.g., `.git`).
    - Expand `skip_names` to include `"git"`, `"refs"`, `"ref"`, `"github"`, `"lib"`, `"bin"`, `"tests"`, `"test"`, `"deps"`, `"build"`, `"dist"`, `"node_modules"`, `"vendor"`, `"quarantine"`, `"tempmediastorage"`.
  - In `bulk_ingest_vault` (specifically the `antigravity` block):
    - Skip scanning any directory whose name starts with a dot `.` or matches `"quarantine"`, `"tempmediastorage"`, `"git"`, `"refs"`, or `"ref"` in a case-insensitive manner.
- **Expected Output**: Git metadata, temporary storage, and reference folders are completely ignored by the scanner and never resolved as scopes.
- **Validation**: Compile succeeds and tests pass.

---

## T6: Ingestion Reset & Full Execution
- **Purpose**: Wipe old legacy files and SurrealDB records to prevent duplication, and re-ingest all episodes under the new 20,000-character, graph-linked architecture.
- **Related Requirements**: A7
- **Related Tests**: None (Manual validation)
- **Inputs**: Shell commands
- **Actions**:
  - Wipe the SurrealDB database.
  - Delete all files inside `/Users/keith/Documents/obsidian-knowledge-graph/Antigravity/episodes/`.
  - Re-run the ingestion pipeline using `./scripts/maintain_mythrax.sh resume-all`.
  - Verify that the active vault `/Users/keith/Documents/obsidian-knowledge-graph/Antigravity/episodes/` has clean, page-level chunked markdown files with correct Obsidian navigation callouts.
- **Expected Output**: Complete, duplicate-free, and sequential page-level chunked episode files in the Obsidian vault.
- **Validation**: Confirm the number of files and inspect their footers.

