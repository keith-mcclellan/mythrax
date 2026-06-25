# Requirements: Page-Level Chunking & Graph Linking

## Problem
The previous chunking limit for document forging (2,000 tokens) was too small, producing too many fragments and creating excessive LLM extraction calls. Conversely, the chunking limit for transcripts (100,000 characters) was too large, causing extremely high prefill latencies (22+ seconds) on local hardware and leading to context exhaustion. Additionally, multi-part transcripts and document chunks lacked robust sequential and hierarchical linking in both the database and the Obsidian vault, leading to out-of-order synthesis.

## Outcome
A consolidated, high-performance chunking architecture targeting **20,000 characters** ($\approx$ 5,000 tokens) that:
1. Splits files cleanly at page, section, paragraph, and line boundaries recursively.
2. Embeds collapsible navigation callouts with explicit wikilinks in Obsidian.
3. Establishes sequential (`next`/`prev`) and hierarchical (`parent`) relationships in SurrealDB.
4. Groups split transcript parts under a shared short UUID and a physical parent index file.

## User Value
- **60% reduction** in document forging LLM overhead and extraction API calls.
- **80% reduction** in local prompt processing prefill latency (down from 22 seconds to ~4.2 seconds).
- Complete structural graph integrity preventing out-of-order memory synthesis or reasoning by AI agents.

## In Scope
- Implementation of a unified, frontmatter-aware recursive chunking algorithm in `chunk_parsed_content`.
- Modification of `bulk_ingest_vault` for the `antigravity` harness:
  - Defaulting to a 20,000-character chunk limit.
  - Generating a single shared UUID suffix for all parts of a transcript.
  - Generating physical parent index files on disk for multi-part episodes.
  - Injecting Obsidian-native collapsible navigation callouts.
  - Downcasting `StorageBackend` to `SurrealBackend` and relating parts sequentially and hierarchically in SurrealDB.
- Modification of `semantic_chunk_text` in `Forge` to call the new 20,000-character chunker.
- Injection of Obsidian-native navigation callouts in Forge chunks, and a Chunks index in parent forged documents.
- Updating test suite files (`test_semantic_document_splitting_relations.rs`, `test_forge.rs`, `test_vault_lifecycle.rs`) to compile and pass under the new 20,000-character limit.

## Out of Scope
- Modifying other ingestion harnesses (`claude`, `cursor`, `hermes`, etc.) that do not perform chunking.
- Adding overlapping context to `chunk_parsed_content`.

## Inputs
- Raw text documents (markdown or extracted text from PDFs/logs).
- Target character limit (20,000 characters).

## Outputs
- Flat lists of clean, boundary-aligned text chunks.
- Markdown files in the Obsidian vault with collapsible navigation headers.
- Bidirectional and parent-child relations in SurrealDB.

## Constraints
- **Absolute Limit**: No chunk may exceed the specified character limit (20,000 characters) unless a single word is larger than the limit, in which case it is split by character.
- **Frontmatter Safety**: The enclosing `---` markers at the start of a markdown file must never be treated as page breaks.

## Risks and Edge Cases
1. **Accidentally shredding frontmatter**: Solved by making the page splitter stateful and frontmatter-aware.
2. **Infinite loops during character fallback**: Solved by decrementing remaining text sizes correctly during word and character splits.
3. **Database downcasting safety**: Ensure that downcasting from `StorageBackend` to `SurrealBackend` fails gracefully if a mock backend is used, avoiding runtime panics.

## Acceptance Criteria
- [ ] **A1 (Limit Enforcement)**: All generated chunks for both Forge and Ingestion must be at most 20,000 characters in length.
- [ ] **A2 (Boundary Alignment)**: Splits must occur at the highest priority boundary available (`\f` or `---` page breaks first, then `#` section headers, then `\n\n` paragraphs, then `\n` lines, and finally whitespace/words).
- [ ] **A3 (Obsidian Navigation Callout)**: Each chunked part/file must append a collapsible `> [!INFO] Navigation` callout at the bottom containing correct `Parent`, `Prev`, and `Next` Obsidian wikilinks.
- [ ] **A4 (Parent Document Indexes)**: Parent Forge documents and multi-part Episode index files must contain a `## Chunks` or `## Parts` section listing wikilinks to all child chunks in correct order.
- [ ] **A5 (SurrealDB Graph Relations)**: SurrealDB must contain bidirectional `relates_to` (`relation: 'next'` and `relation: 'prev'`) edges between adjacent chunks, and `relation: 'parent'` edges pointing to the parent document/episode record.
- [ ] **A6 (Shared Episode UUID)**: All chunked parts of a single transcript must share the exact same short UUID suffix in their filenames.
- [ ] **A7 (Pristine Ingestion Reset)**: Running a reset wipes both the SurrealDB database and the `episodes/` folder on disk to ensure no legacy 100k files remain.
- [ ] **A8 (Test Suite Compliance)**: The entire integration test suite (`cargo test`) compiles and passes cleanly with 100% success.
- [ ] **A9 (Scope-Based Folder Organization)**: All forged documents (parent, chunks, concepts) must be saved to `wiki/{scope}/` and all forged rules to `wisdom/{scope}/`, where `scope` is normalized to lowercase/alphanumeric and defaults to `general` if empty, matching the episode ingestion folder structure.
- [ ] **A10 (Phantom Project Prevention)**: The scope resolution logic (`resolve_scope_from_path`) must skip any path components starting with `.` (e.g., `.git`, `.github`) and generic names (e.g., `git`, `refs`, `ref`, `lib`, `bin`, `tests`). The bulk ingestion scanner must also ignore hidden directories and metadata folders in the source directory.


