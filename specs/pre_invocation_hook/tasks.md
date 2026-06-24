# Tasks: Mythrax Pre-Invocation Hook

## T1: MCP Tool Declaration
- **Purpose**: Register the new MCP tool `pre_invocation_hook` under the `mythrax` server tools.
- **Related Requirements**: In Scope (MCP Tool declaration).
- **Related Tests**: `test_pre_invocation_hook_flow`.
- **Inputs**: None.
- **Actions**:
  - Open [mcp.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/mcp.rs).
  - Locate `get_tools` tool definition array (around line 240-270).
  - Add the `pre_invocation_hook` tool definition, specifying its description and input schema (`session_id` optional string, `query` optional string, `workspace_path` optional string).
- **Expected Output**: The MCP server exposes `pre_invocation_hook` when `list_tools` is requested.
- **Validation**: Inspect compile errors.

## T2: Hook Handler Implementation in mcp.rs
- **Purpose**: Implement the backend logic for state synchronization, subagent handoff detection, direct context node hydration, abstraction priority traversal, sibling HTR constraints, and root agent hybrid retrieval.
- **Related Requirements**: In Scope (Passive Sync, Subagent Path, Direct Hydration, Root Agent Path, Active HTR Context, Abstraction Priority, Distilled Memory Cards).
- **Related Tests**: `test_pre_invocation_hook_flow` and all sub-scenarios.
- **Inputs**: `session_id`, `query`, `workspace_path`.
- **Actions**:
  - Open [mcp.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/mcp.rs).
  - Locate `call_tool_internal` match block (around line 890-910).
  - Add `pre_invocation_hook` case matching.
  - Implement **Step 1: Passive Synchronization** (call `journal_state` and `verify_vault_integrity`).
  - Implement **Step 2: Subagent Detection** (query `handoff` table for matching `subagent_conversation_id`).
  - Implement **Step 3: Subagent Direct Hydration / Fallback Search**:
    - Load handoff and STM key-values.
    - If `distilled_context_nodes` exists in STM: parse node IDs, query database, and hydrate. Skip semantic search.
    - If absent: run fallback search over `wisdom` and `memories` using handoff summary.
  - Implement **Step 4: Root Agent Path**:
    - Resolve active project scope name from `workspace_path` folder.
    - Perform semantic search over `wisdom` (threshold `0.55`).
    - Perform semantic search over `memories` (threshold `0.70`).
    - If `memories` is empty, append the pinned deep-search instruction.
  - Implement **Step 5: Active HTR Context & Sibling Negative Constraints**:
    - Resolve the active hypothesis node.
    - Query database for immediate failed/pruned siblings (`parent_id = active_parent_id`).
    - Append hypothesis statements and critic insights as negative constraints.
  - Implement **Step 6: Distilled Memory & Abstraction Priority Formatting**:
    - Distilled nodes (WikiNodes, WisdomRules, HypothesisNodes) are fully rendered.
    - Check if raw episodes have upward `relates_to` links to parent `wiki_node` insights. If so, inject the parent insight.
    - Otherwise, render episodes as compact cards with standardised footnotes.
  - Handle database connection errors gracefully and return a fallback warning string instead of crashing.
- **Expected Output**: The handler compiles successfully and returns a detailed markdown report based on database state.
- **Validation**: Inspect compile errors.

## T3: Main Harness Hook Configuration
- **Purpose**: Update `merge_antigravity_hooks` in `main.rs` to register `pre_invocation_hook` in the harness's `hooks.json`.
- **Related Requirements**: In Scope (Harness hook config).
- **Related Tests**: `test_cli_e2e`.
- **Inputs**: `hooks.json` path, `exe_path`.
- **Actions**:
  - Open [main.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/main.rs).
  - Locate `merge_antigravity_hooks` function (around line 1286).
  - Modify the JSON insertion of `PreInvocation` to contain both `verify_compliance` and `pre_invocation_hook` MCP tool hooks:
    ```json
    [
      {
        "type": "mcp",
        "server": "mythrax",
        "tool": "verify_compliance"
      },
      {
        "type": "mcp",
        "server": "mythrax",
        "tool": "pre_invocation_hook"
      }
    ]
    ```
- **Expected Output**: `mythrax config` registers both hooks in `hooks.json`.
- **Validation**: Check that compile passes.

## T4: Active Global Configuration Update
- **Purpose**: Update the active global `/Users/keith/.gemini/config/hooks.json` file to include `pre_invocation_hook` immediately.
- **Related Requirements**: In Scope (Active config update).
- **Related Tests**: Manual check.
- **Inputs**: None.
- **Actions**:
  - Open and modify `/Users/keith/.gemini/config/hooks.json` to include the `pre_invocation_hook` tool case in `PreInvocation` array.
- **Expected Output**: File updated correctly.
- **Validation**: View the file to verify.

## T5: Write Integration Tests in test_stm.rs
- **Purpose**: Implement the full suite of integration tests in `test_stm.rs` to verify root agent, subagent, direct hydration, fallback search, HTR negative constraints, and abstraction priority scenarios.
- **Related Requirements**: Acceptance Criteria (AC-1 through AC-9).
- **Related Tests**: `test_pre_invocation_hook_flow`.
- **Inputs**: None.
- **Actions**:
  - Open [test_stm.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/tests/test_stm.rs).
  - Add the new `test_pre_invocation_hook_flow` test function.
  - Implement all mock database state insertions and tool invocation test blocks as specified in the `test-plan.md`.
- **Expected Output**: Running `cargo test` executes the test suite and passes all assertions.
- **Validation**: Run `cargo test --manifest-path mythrax-core/Cargo.toml --test test_stm`.
