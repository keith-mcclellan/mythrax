# Clarify: Paragraph-Boundary and Code-Object aware Chunking

## Restated Request
Refine the chunking algorithm (`chunk_parsed_content`) used across episodes, artifacts, and document forging so that instead of splitting arbitrarily by line or character boundaries at exactly 100k, it respects paragraph boundaries (double newlines `\n\n`) and code/structural object boundaries (such as functions/classes separated by blank lines) to avoid shredding key context/insights.

## Known Facts
1. The function `chunk_parsed_content(content: &str, limit: usize)` in `mythrax-core/src/vault/ingestion.rs` is the common chunking logic.
2. It is called by:
   - Episode ingestion.
   - Raw artifact ingestion.
   - Document forging (second-pass logical section chunking).
3. The current implementation splits strictly line-by-line (`content.lines()`), and if a single line is too long, it splits arbitrarily by character index (`split_at(limit)`). This can split in the middle of a paragraph or code block if a line-boundary happens to be within a larger context block.
4. Double newlines `\n\n` in Markdown and plain text represent paragraphs.
5. In raw code files (Python, Rust, JS, etc.), double newlines `\n\n` (or multiple newlines) are typically used to separate functions, classes, and other major code structures.

## Assumptions
1. Double newlines (`\n\n`) are the primary boundary delimiter for paragraphs and code blocks.
2. Normalizing `\r\n` to `\n` before processing simplifies boundary detection without affecting semantic correctness.
3. If a paragraph/code block is too large to fit within a single chunk (exceeds the 100k character limit), we should fall back to splitting it by lines (`\n`).
4. If an individual line is still too large to fit within a single chunk, we should fall back to splitting it by character index.
5. This hierarchical fallback ensures that the chunking remains robust for all input lengths while maximizing coherence.

## Ambiguities & Tradeoffs
1. *Overlap*: The current `chunk_parsed_content` does not use overlap (unlike `chunk_text` in forge). We will keep this behavior (no overlap) to avoid modifying the caller contracts, but split cleanly at boundaries.
2. *Minification*: Minified code/data (e.g., a huge single-line JSON or minified JS file) won't have double newlines or single newlines. The line-based and character-based fallback will handle these cases gracefully.

## Blocking Questions
None. The hierarchical fallback design covers all constraints.
