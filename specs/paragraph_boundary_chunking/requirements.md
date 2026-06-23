# Requirements: Paragraph-Boundary and Code-Object aware Chunking

## Problem
The current chunking implementation in `chunk_parsed_content` splits content strictly at single-line boundaries. If a paragraph or code structure spans multiple lines, it can easily be cut in half if it crosses the 100k character boundary, shredding structural context and reducing search/indexing effectiveness.

## Outcome
A more intelligent chunking function that chunks primarily at paragraph/code block boundaries (`\n\n`), falling back to line boundaries (`\n`), and finally to character boundaries only when necessary.

## User Value
- Ensures key insights, code objects, and cohesive thoughts remain in the same chunk.
- Enhances vector search precision since semantic units (paragraphs, functions) are preserved whole.
- Minimizes context fragmentation.

## In Scope
- Modify `chunk_parsed_content` to use a hierarchical splitting strategy:
  1. Split input by paragraph boundaries (`\n\n` after normalizing `\r\n` to `\n`).
  2. If a paragraph exceeds the limit, split it by line boundaries (`\n`).
  3. If a single line exceeds the limit, split it by character index.
- Maintain existing function signature: `pub fn chunk_parsed_content(content: &str, limit: usize) -> Vec<String>`.
- Add comprehensive test coverage to verify correct splitting behavior at each fallback tier.

## Out of Scope
- Adding overlapping chunking to `chunk_parsed_content`.
- Modifying other chunking strategies like the token-based `chunk_text` function unless needed.

## Inputs
- `content: &str`: The full text to chunk.
- `limit: usize`: The maximum character limit per chunk.

## Outputs
- `Vec<String>`: The list of chunks, each strictly within `limit` characters (unless the character fallback is required, which still obeys the limit).

## Constraints
- Safe budget limit of 100,000 characters.
- Must preserve all text content (no content lost or altered other than newline normalization).

## Acceptance Criteria
1. **Paragraph Preservation**: Inputs with multiple paragraphs (separated by `\n\n`) where paragraphs fit within the limit must be split at `\n\n` boundaries, never in the middle of a paragraph.
2. **Line Fallback**: If a paragraph exceeds the limit, it must be split at line breaks (`\n`), keeping lines intact where possible.
3. **Character Fallback**: If a line exceeds the limit, it must be split at the character limit.
4. **All Tests Pass**: The existing tests in `test_vault_lifecycle.rs` and `test_forge.rs` must pass without modifications to their core assertions.
