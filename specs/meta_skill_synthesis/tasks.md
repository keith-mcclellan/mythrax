# Tasks: Meta-Skill Synthesis

## T1: Add `get_all_wiki_nodes` to Database Backend
- **Purpose**: Allow retrieval of all forged documents (Wiki Nodes) from SurrealDB.
- **Related Requirements**: Functional scope.
- **Related Tests**: `test_get_all_wiki_nodes` in `backend.rs`.
- **Inputs**: None.
- **Actions**:
  - Add `async fn get_all_wiki_nodes(&self) -> Result<Vec<WikiNode>>` to `StorageBackend` trait in [backend.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/db/backend.rs).
  - Implement it in `SurrealBackend` in [backend.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/db/backend.rs).
- **Expected Output**: Compiles successfully.

## T2: Implement Meta-Skill Synthesizer & Merging Engine
- **Purpose**: Group rules/docs/skills by scope, synthesize playbooks, and support merging playbooks with automatic opportunity detection.
- **Related Requirements**: Functional scope, Acceptance Criteria 1, 2, 4.
- **Related Tests**: `test_meta_skill_synthesis`, `test_detect_skill_merges`, `test_execute_skill_merge`.
- **Inputs**: SurrealDB and `MarkdownStore`.
- **Actions**:
  - Create [meta_skill.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/cognitive/meta_skill.rs).
  - Declare `pub mod meta_skill;` in [mod.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/cognitive/mod.rs).
  - Implement scope-based synthesis, `detect_skill_merges`, and `merge_skills` in `meta_skill.rs`.
- **Expected Output**: Synthesizer and merging workflows implemented.

## T3: Register MCP Tools
- **Purpose**: Expose synthesis, detection, and merge execution to the agent.
- **Related Requirements**: Acceptance Criterion 3.
- **Related Tests**: None.
- **Inputs**: MCP schema.
- **Actions**:
  - Register `synthesize_meta_skills`, `detect_skill_merges`, and `merge_skills` schemas in [mcp.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/mcp.rs).
  - Implement the handlers in [mcp.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/mcp.rs) to call the synthesizer methods.
- **Expected Output**: Three new MCP tools available.

## T4: Add Integration and Unit Tests
- **Purpose**: Verify end-to-end functionality of synthesis, detection, and merging.
- **Related Requirements**: Acceptance Criteria 1, 2, 3, 4.
- **Related Tests**: All new tests.
- **Inputs**: Test framework.
- **Actions**:
  - Add `test_get_all_wiki_nodes` in [backend.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/db/backend.rs).
  - Create [test_meta_skill.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/tests/test_meta_skill.rs) integration test implementing unit tests for all synthesis/merge methods.
- **Expected Output**: Tests compile and pass.
