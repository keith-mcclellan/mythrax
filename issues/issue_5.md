# Bug: Missing test coverage for code substitution logic
**Labels**: bug, agent-found

**File**: `mythrax-core/src/cognitive/paging.rs`
**Line**: 72, 103
**Severity**: Low

**Scenario**:
The public functions `extract_symbols` and `page_code_block` lack test coverage. They are critical for agent virtual paging functions, introducing risks of silent extraction and code substitution failures if modified.

**Suggested Fix**:
Add comprehensive unit tests in a `mod tests` block covering `extract_symbols` and `page_code_block` for various languages, edge cases, and substitution behaviors.