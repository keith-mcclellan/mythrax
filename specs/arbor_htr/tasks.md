# Tasks - HTR Loop Generalization & CLI/MCP Integration

## T1: Update Database Schema & HypothesisNode Struct
- **Purpose**: Prepare database and memory structures to support dynamic HTR data, specifically the `code_changes` map.
- **Related Requirements**: In Scope 1
- **Related Tests**: `test_hypothesis_node_serialization`
- **Inputs**: `src/contracts.rs`, `src/db/schema.rs`
- **Actions**:
  1. Add `code_changes`, `scope`, and `vault_path` fields to `HypothesisNode` struct in `contracts.rs`.
  2. Update `INIT_SCHEMA` in `db/schema.rs` to include the correct fields for `hypothesis_node` table (specifically `code_changes` as option object).
- **Expected Output**: Structures compile and database schemas allow serialization of HTR nodes with code changes.
- **Validation**: Confirm `cargo test` baseline passes.

## T2: Generalize ArborExecutor
- **Purpose**: Allow `ArborExecutor` to write dynamic file changes before executing tests in the sandboxed git worktree.
- **Related Requirements**: In Scope 2
- **Related Tests**: `test_executor_applies_code_changes`
- **Inputs**: `src/cognitive/executor.rs`
- **Actions**:
  1. Modify `ArborExecutor::execute` signature to accept `code_changes: &Option<HashMap<String, String>>`.
  2. In `execute`, if `code_changes` is present, iterate over the map and write the string values to their corresponding relative file paths inside the temp worktree directory.
- **Expected Output**: Worktree contains the generated codebase modifications before running tests.
- **Validation**: Write and run a unit test verifying file generation inside the worktree.

## T3: Generalize ArborCoordinator & LLM Client Integration
- **Purpose**: Remove hardcoded prime checker logic, parameterize inputs, and connect the LLM client prompts to dynamic changes.
- **Related Requirements**: In Scope 2, 3
- **Related Tests**: `test_generalized_arbor_htr_loop_lifecycle`
- **Inputs**: `src/cognitive/arbor.rs`, `src/llm/mod.rs`
- **Actions**:
  1. Update `ArborCoordinator` struct and constructor to accept `scope: String`, `test_command: String`, and `target_files: Vec<String>`.
  2. Refactor `init_root` to accept `hypothesis: String` and optional `code_changes`.
  3. Refactor `trigger_ideation` to parse the proposed child `code_changes` map and write them to the DB and vault notes.
  4. Refactor `execute_node` to pass the node's `code_changes` map to the executor and run the coordinator's dynamic `test_command`.
  5. Refactor `decide_admission` to apply the selected node's `code_changes` to the main branch files and run a git commit.
  6. Update `LLMClient` prompts in `src/llm/mod.rs` to request a JSON response containing `code_changes` for each proposed hypothesis. Include contents of `target_files` in the ideation prompt context.
- **Expected Output**: The coordinator and LLM prompts are completely generic.
- **Validation**: Compile and run the updated integration test `test_arbor_htr_loop_lifecycle.rs` using Mock LLM inputs.

## T4: Register CLI Subcommands & Expose MCP Tools
- **Purpose**: Wire HTR capabilities to the command line and agent interfaces.
- **Related Requirements**: In Scope 4, 5
- **Related Tests**: `test_cli_htr_run_command`, `test_mcp_htr_tools`
- **Inputs**: `src/cli.rs`, `src/main.rs`, `src/mcp.rs`
- **Actions**:
  1. Register `Htr` commands in `cli.rs` (`Init`, `Ideate`, `Execute`, `Backprop`, `Merge`, `Run`).
  2. Wire these subcommands to `main.rs` to construct an `ArborCoordinator` and execute the corresponding method.
  3. In `main.rs`, implement the loop for `mythrax htr run` to execute phases end-to-end.
  4. In `mcp.rs`, register corresponding tools and map them inside `call_tool`.
- **Expected Output**: HTR loop commands are accessible to users and agents.
- **Validation**: Verify `mythrax htr --help` works and lists all subcommands, and verify mcp schemas.
