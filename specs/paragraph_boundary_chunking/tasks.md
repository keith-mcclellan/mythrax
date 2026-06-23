# Tasks: Paragraph-Boundary and Code-Object aware Chunking

## T1: Implement new chunking logic
- **Purpose**: Implement the hierarchical paragraph/line/character chunking algorithm in `chunk_parsed_content`.
- **Related Requirements**: Acceptance Criteria 1, 2, 3.
- **Related Tests**: `test_chunk_parsed_content_simple`, `test_chunk_parsed_content_paragraph_boundary`, `test_chunk_parsed_content_line_fallback`, `test_chunk_parsed_content_character_fallback`.
- **Inputs**: `content: &str`, `limit: usize`.
- **Actions**: Replace current implementation of `chunk_parsed_content` in `mythrax-core/src/vault/ingestion.rs` with the new hierarchical algorithm.
- **Expected Output**: Content is split into chunks of max `limit` characters at paragraph boundaries first, then lines, then characters.
- **Validation**: Successful compile.

## T2: Add unit tests
- **Purpose**: Assert that `chunk_parsed_content` splits correctly at paragraph, line, and character boundaries.
- **Related Requirements**: Acceptance Criteria 1, 2, 3.
- **Related Tests**: The new unit tests.
- **Inputs**: None.
- **Actions**: Add the unit tests to `mythrax-core/src/vault/ingestion.rs` under `mod tests`.
- **Expected Output**: Tests compile and pass.
- **Validation**: Run `cargo test` and verify tests pass.

## T3: Verify full suite
- **Purpose**: Ensure no regressions in existing ingestion and forge chunking tests.
- **Related Requirements**: Acceptance Criterion 4.
- **Related Tests**: All existing tests.
- **Inputs**: None.
- **Actions**: Run `cargo test` for the entire crate.
- **Expected Output**: All tests pass.
- **Validation**: Verification results in terminal.
