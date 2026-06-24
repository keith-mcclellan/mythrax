# Test Plan: Mythrax Pre-Invocation Hook

We will verify the pre-invocation hook's behavior using Rust integration tests inside `mythrax-core/tests/test_stm.rs` (or a dedicated integration test file).

## Unit Tests
- `test_workspace_scope_derivation`: Verifies that the active scope is correctly extracted from the workspace path (e.g. `"/path/to/my-project"` -> `"my-project"`, and invalid paths fallback to `"general"`).
- `test_episode_card_formatting`: Verifies that raw episodes are correctly formatted as compact cards with direct hydration footnotes, and that distilled insights/wiki nodes are rendered with full content.

## Integration Tests
We will add a comprehensive test case `test_pre_invocation_hook_flow` to `mythrax-core/tests/test_stm.rs` covering the following scenarios:

### 1. Root Agent Flow (Dynamic Retrieval)
- **Setup**:
  - Save a mock wisdom rule (similarity `0.60` to query).
  - Save a mock high-confidence episode memory (similarity `0.80` to query).
  - Save a mock low-confidence episode memory (similarity `0.60` to query).
- **Execution**:
  - Call the `pre_invocation_hook` MCP tool with `query` and a root `session_id` (no handoff).
- **Assertions**:
  - Verify the status is reported as Root Agent.
  - Verify the wisdom rule is retrieved and fully rendered.
  - Verify the high-confidence episode is retrieved as a compact metadata card.
  - Verify the low-confidence episode is **not** retrieved.
  - Verify the pinned deep-search instruction is **not** injected (since a high-confidence memory was found).

### 2. Pinned Instruction Fallback Flow
- **Setup**:
  - Save a mock wisdom rule (similarity `0.60` to query).
  - No high-confidence memories (only the `0.60` low-confidence memory).
- **Execution**:
  - Call the `pre_invocation_hook` MCP tool with `query` and a root `session_id`.
- **Assertions**:
  - Verify the wisdom rule is retrieved.
  - Verify no memories are returned.
  - Verify the **pinned fallback instruction** is successfully injected into the report.

### 3. Subagent Direct Hydration Flow
- **Setup**:
  - Create a parent and subagent session.
  - Save a handoff contract (`save_handoff`) linking parent and subagent.
  - Save a mock WikiNode and a mock raw episode in the database.
  - Stash their record IDs in STM under `"distilled_context_nodes"`.
- **Execution**:
  - Call the `pre_invocation_hook` MCP tool with the subagent `session_id` and **no query**.
- **Assertions**:
  - Verify the status is reported as Subagent, showing parent ID and handoff metadata.
  - Verify all stashed STM key-values are returned.
  - Verify that the specific WikiNode is hydrated and fully rendered.
  - Verify that the specific raw episode is hydrated and rendered as a compact metadata card.
  - Verify that **no semantic search is executed** (bypassing search because distilled nodes were present).

### 4. Subagent Fallback Search Flow
- **Setup**:
  - Create a handoff linking parent and subagent, with a specific handoff summary.
  - No `"distilled_context_nodes"` stashed in STM.
  - Save a mock wisdom rule matching the handoff summary.
- **Execution**:
  - Call the `pre_invocation_hook` MCP tool with the subagent `session_id` and **no query**.
- **Assertions**:
  - Verify the status is reported as Subagent.
  - Verify that the hook executes a fallback semantic search using the **handoff summary** as the query.
  - Verify that the matching wisdom rule is retrieved and returned.

### 5. HTR Sibling Constraints Flow
- **Setup**:
  - Create a parent hypothesis node `hypothesis_node:parent_123`.
  - Create a child hypothesis node `hypothesis_node:child_456` (active node) and a failed sibling node `hypothesis_node:sibling_failed` (status = `'failed'`, parent_id = `'hypothesis_node:parent_123'`) with a mock critic insight.
  - Link `hypothesis_node:child_456` to the subagent's stashed context nodes in STM.
- **Execution**:
  - Call the `pre_invocation_hook` MCP tool for the subagent.
- **Assertions**:
  - Verify the active HTR context is resolved.
  - Verify that the failed sibling node is retrieved.
  - Verify that the output contains the **`### Active HTR Negative Constraints`** section, detailing the failed sibling hypothesis and its critic failure insights.

### 6. Abstraction Priority Flow
- **Setup**:
  - Save a raw episode `episode:test_ep`.
  - Save a distilled `wiki_node:distilled_ep`.
  - Link them in SurrealDB via `relates_to`: `RELATE episode:test_ep -> relates_to -> wiki_node:distilled_ep`.
- **Execution**:
  - Call the `pre_invocation_hook` tool in a scenario that retrieves `episode:test_ep`.
- **Assertions**:
  - Verify that the hook detects the upward link and injects the full content of `wiki_node:distilled_ep` **instead** of the compact card for `episode:test_ep`.

## Edge Cases
- **Empty STM / Handoff Table**: Hook runs for a subagent but no STM keys exist. Must return handoff metadata only.
- **Invalid Node IDs in STM**: `"distilled_context_nodes"` contains IDs like `wiki_node:nonexistent`. Must ignore them and not crash.
- **Scope Naming Sanity**: Workspace folder is named `src` or `.git`. Must resolve to `"general"`.

## Failure Modes
- **SurrealDB Connection Offline**: The hook catches the error and returns a clean warning string. The agent turn proceeds.
