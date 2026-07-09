---
labels: bug, agent-found
---

# Test Coverage Gap: `scan_all_skills` has no test coverage

## Location
- File: `mythrax-core/src/cognitive/meta_skill.rs`
- Function: `pub fn scan_all_skills`

## Description
The public function `scan_all_skills(store: &MarkdownStore) -> Vec<SkillInfo>` in `meta_skill.rs` parses directories for `SKILL.md` files (using `~/.gemini/config/skills` and `.agents/skills`). However, `grep` analysis indicates there are no corresponding unit tests in `mythrax-core/tests/` or inline tests that call this function.

## Minimal Reproducible Scenario
1. Modify `scan_all_skills` to introduce a panic or logical flaw (e.g. `unwrap` failing on invalid paths).
2. Run `cargo test` in `mythrax-core`.
3. The test suite passes without hitting the modified code, proving a lack of coverage.

## Severity
Medium. This function touches the file system based on environment variables (`HOME`) and internal structures. Lack of tests means changes here can break agent skill discovery silently.

## Suggested Fix
Create a unit test in a new or existing test module (e.g., `test_meta_skills.rs`) that:
1. Mocks a temporary `HOME` directory and `MarkdownStore`.
2. Creates dummy `SKILL.md` files.
3. Calls `scan_all_skills` and asserts it successfully discovers and parses the files without panicking.