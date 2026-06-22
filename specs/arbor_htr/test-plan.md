# Test Plan - HTR Loop Generalization & CLI/MCP Integration

## Unit Tests
- **`test_hypothesis_node_serialization`**: Assert that `HypothesisNode` with `code_changes` map serializes and deserializes correctly to/from JSON and SurrealDB.
- **`test_executor_applies_code_changes`**: Assert that `ArborExecutor` correctly writes a mock map of `code_changes` to the temporary git worktree folder before executing a shell command.

## Integration Tests
- **`test_generalized_arbor_htr_loop_lifecycle`**: Update the existing integration test `test_arbor_htr_loop_lifecycle.rs` to use the generalized coordinator and dynamic code changes instead of the hardcoded prime sieve example. Validate:
  - Root initialization with custom hypothesis and codebase.
  - Ideation proposal of code edits.
  - Correct execution of the dynamic code changes inside the git worktree.
  - Backpropagation of evaluation scores and abstraction of insights.
  - Successful merge of selected code changes back to the mock repository.
- **`test_schema_initialization`**: Validate that initializing SurrealDB using `INIT_SCHEMA` sets up the schema for `hypothesis_node` correctly and allows saving `HypothesisNode` records with dynamic code changes.

## Acceptance Tests
- **`test_cli_htr_run_command`**: Execute the end-to-end HTR run subcommand via CLI (or integration test simulating the CLI parser) to verify it initializes, runs ideation, executes, backpropagates, and merges code edits dynamically.
- **`test_mcp_htr_tools`**: Verify that calling the registered HTR MCP tools returns valid JSON-RPC responses and matches the CLI behavior.

## Edge Cases
- **Empty `code_changes`**: If a hypothesis node proposes no code changes, verify it executes successfully using the parent node's codebase.
- **Missing model files fallback**: Verify that LLM client fallbacks and coordinator error handling behave gracefully when ONNX models are missing.

## Failure Modes
- **Syntax Error in LLM Response**: Proposing malformed JSON. Assert that the ideation step fails gracefully with a parsing warning rather than a panic.
- **Dirty git repository during merge**: Merging code when the repository is dirty. Assert that a clean error message is returned.
