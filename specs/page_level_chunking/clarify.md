# Clarify: Page-Level Chunking & Graph Linking

## Restated Request
Evolve the Mythrax memory architecture to support page-level semantic chunking and comprehensive graph linking across both the document forging (`Forge`) and transcript ingestion (`bulk_ingest_vault`) pipelines.
- **Chunk Size**: Consolidate maximum chunk size to **20,000 characters** ($\approx$ 5,000 tokens) instead of 2,000 tokens (for Forge) or 100,000 characters (for episodes).
- **Boundaries**: Honor page breaks (`\f`, `---`/`***`/`___`), section headers (lines starting with `#`), paragraphs (`\n\n`), and lines (`\n`) recursively.
- **Graph Coherence**: Establish bidirectional `next`/`prev` and `parent`/`child` graph relations in both SurrealDB and Obsidian markdown files (using callouts and wikilinks).
- **Orchestration**: Provide extremely thorough atomic tasks with AST function headers so the local model can execute surgical code changes without doing design work.

## Known Facts
1. **Forge Chunker**: Located in `mythrax-core/src/cognitive/forge.rs`. The method `semantic_chunk_text` is the entry point for document chunking.
2. **Ingestion Chunker**: Located in `mythrax-core/src/vault/ingestion.rs`. The function `chunk_parsed_content` is the entry point for log and artifact chunking.
3. **SurrealDB Backend**: `SurrealBackend` is located in `mythrax-core/src/db/backend.rs`. It implements the `StorageBackend` trait. We can downcast the `db: &dyn StorageBackend` to `SurrealBackend` using `db.as_any().downcast_ref::<SurrealBackend>()` to run raw queries/relations.
4. **Local LLM Performance**: Prefill time is linear at ~0.85 ms per token. A 20k character limit (5,000 tokens) results in a highly responsive prefill latency of ~4.2 seconds on local hardware compared to 22 seconds for 100k characters.

## Assumptions
1. **Clean Reset**: To avoid duplication between old 100k/2k-token files and new 20k-character files, we will perform a clean database reset and clear out the `episodes/` folder in the vault prior to re-ingestion.
2. **First-part/Single-part Standalone**: If a transcript fits fully within 20,000 characters, it is saved directly as a single standalone file, and no virtual parent or parts are generated.
3. **No Overlap**: Clean, non-overlapping page-aligned splits will be performed to maximize context clarity and avoid duplication.

## Tradeoffs
- **Smaller Contexts vs. Parallel Calls**: Moving to a 20k character limit for episodes increases the number of chunks for massive files, meaning more sequential LLM calls. However, this is heavily offset by the exponential/linear reduction in prompt prefill times (4.2s vs. 22s), preventing local GPU memory exhaustion and improving throughput.
- **Wikilinks in Obsidian vs. Markdown Cleanliness**: Adding parent/prev/next navigation footers adds markdown text, but wrapping them in collapsible Obsidian callouts (`> [!INFO] Navigation`) keeps the main body clean for humans while retaining high semantic value for agents.
