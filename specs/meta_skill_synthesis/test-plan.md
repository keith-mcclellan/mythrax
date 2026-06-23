# Test Plan: Meta-Skill Synthesis

## Unit Tests
1. **`test_get_all_wiki_nodes`** in `backend.rs`:
   - Save two wiki nodes in SurrealDB.
   - Retrieve all wiki nodes using `get_all_wiki_nodes()`.
   - Assert that they are correctly returned and their Record IDs are formatted properly.

2. **`test_meta_skill_synthesis`** in `tests/test_meta_skill.rs` (new test file):
   - Seed SurrealDB with:
     - Two wisdom rules with similar content (e.g. database transactions).
     - One wiki node with similar content (e.g. database schema details).
   - Mock LLM calls (using `MYTHRAX_MOCK_LLM=true`).
   - Run the synthesizer.
   - Assert that a skill directory is created under a temp `.agents/skills` directory, containing a valid `SKILL.md` file.

3. **`test_detect_skill_merges`** in `tests/test_meta_skill.rs`:
   - Create two temp playbooks on disk with highly similar description embeddings.
   - Run merge opportunity detection.
   - Assert that the pair is flagged as a merge candidate with high similarity.

4. **`test_execute_skill_merge`** in `tests/test_meta_skill.rs`:
   - Create two temp playbooks on disk: `meta-git-commit/SKILL.md` and `meta-git-pull/SKILL.md`.
   - Call the merge execution method to combine them into `meta-git/SKILL.md`.
   - Assert that `meta-git/SKILL.md` is successfully created, and the source directories `meta-git-commit/` and `meta-git-pull/` are deleted.

## Integration Tests
- Run `cargo test` to ensure no regressions in existing cognitive loops (DBSCAN, compactor, harvester).
- Verify that the MCP tools `synthesize_meta_skills`, `detect_skill_merges`, and `merge_skills` are successfully registered.
