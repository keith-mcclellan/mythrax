# Test Plan: Paragraph-Boundary and Code-Object aware Chunking

## Unit Tests
We will add unit tests inside `mythrax-core/src/vault/ingestion.rs` (under `mod tests`) to verify the chunking algorithm directly:
1. **`test_chunk_parsed_content_simple`**: A small string under the limit should return a single chunk.
2. **`test_chunk_parsed_content_paragraph_boundary`**: A string with multiple paragraphs where a split is needed, verifying it splits exactly at `\n\n` and does not break inside a paragraph.
3. **`test_chunk_parsed_content_line_fallback`**: A string with a single paragraph that exceeds the limit, verifying it falls back to splitting at line boundaries (`\n`) and does not split inside a line.
4. **`test_chunk_parsed_content_character_fallback`**: A string with a single long line exceeding the limit, verifying it falls back to character splitting.

## Integration Tests
Run all existing integration tests:
1. **`test_second_pass_character_chunking`** in `test_forge.rs`.
2. **`test_ingestion_chunking_and_linking`** in `test_vault_lifecycle.rs`.
3. **`test_artifact_chunking_during_ingestion`** in `test_vault_lifecycle.rs`.

All tests must compile and pass cleanly.
