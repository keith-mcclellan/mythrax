# Requirements - HTR Loop Generalization & CLI/MCP Integration

## Problem
The Hypothesis-Tree Refinement (Arbor) loop is currently a mock-like library component. It has hardcoded file names, hardcoded test commands, hardcoded code changes, and is completely isolated from the outside world (no CLI subcommands, REST endpoints, or MCP tools). To become a true "generalist autonomous research engine," it must be parameterized, connected to the active LLM provider configurations, and exposed to other developer tools and AI systems.

## Outcome
A generic and dynamic HTR implementation within the unified Rust core that supports:
1. Creating a persistent hypothesis tree with dynamic initial codebases.
2. Generating dynamic hypotheses and code refinements using the production LLM client.
3. Running arbitrary test commands in isolated sandboxed git worktrees and evaluating outcomes via LLM-driven criticism.
4. Auto-merging the best-performing code refinements into the main branch.
5. Invoking the HTR loop both end-to-end and step-by-step via CLI subcommands and MCP tools.

## User Value
- Allows developers and agents to run complex code optimizations and search for correct implementations autonomously.
- Integrates the HTR research loop directly into IDEs via MCP and CLI interfaces.
- Protects the main repository from dirty code changes by evaluating all refinements in sandboxed git worktrees.

## In Scope
1. **Dynamic HypothesisNode**: Add a `code_changes` field (mapping relative file paths to proposed file content) to the `HypothesisNode` structure.
2. **Generic ArborCoordinator & ArborExecutor**:
   - Parameterize coordinator with dynamic `scope`, `test_command`, and `target_files`.
   - Update `ArborExecutor` to apply target file changes directly inside the worktree before executing the test command.
3. **Production LLM Client Integration**:
   - Update `ArborLlmClient` prompts in `LLMClient` to dynamically request hypotheses, expected scores, and corresponding target code changes.
   - Inject the production `LLMClient` into the coordinator during execution.
4. **CLI Subcommand Interface**:
   - Add `htr` command group to `mythrax` with subcommands:
     - `mythrax htr init --scope <scope> --hypothesis <text> --files <f1,f2,...>`
     - `mythrax htr ideate --scope <scope> --node <parent_id>`
     - `mythrax htr execute --scope <scope> --node <node_id> --test-command <cmd>`
     - `mythrax htr backprop --scope <scope> --node <node_id>`
     - `mythrax htr merge --scope <scope> --node <node_id>`
     - `mythrax htr run --scope <scope> --hypothesis <text> --files <f1,f2,...> --test-command <cmd> [--max-steps <n>]`
5. **MCP Server Integration**:
   - Register equivalent MCP tools for each low-level phase and the end-to-end execution.

## Out of Scope
- Parallel execution of multiple HTR runs in conflicting git worktrees (execution is sequential per coordinate).
- Automatically running HTR loops without a specified test command.

## Inputs
- `scope`: The workspace target scope for database/vault namespaces.
- `hypothesis`: The objective or refinement description.
- `target_files`: A list of relative file paths to be evaluated and modified.
- `test_command`: The command to execute tests (e.g. `npm test`, `pytest`).

## Outputs
- Structured SurrealDB records in table `hypothesis_node`.
- Markdown files in `wiki/<scope>/hypothesis_tree/<node_id>.md`.
- Merged code changes committed directly to the main git repository.

## Constraints
- **Zero code pollution**: Under no circumstances should failed or unmerged code changes be left in the main repository.
- **Git Worktree Cleanup**: Temporary worktrees must be forcefully removed (`git worktree remove --force`) and branches deleted upon execution failure or success.

## Assumptions
- The active provider in `keys.json` or environment variables is configured and holds enough tokens to run prompts.
- Git is installed and initialized in the target repository.

## Risks and Edge Cases
1. **Model Syntax Errors**: The LLM might propose invalid JSON or malformed code replacements.
   - *Mitigation*: Fallback to parse gracefully, log errors, and skip node execution if code changes cannot be extracted.
2. **Git Worktree Lock**: Git worktree locking if process is terminated unexpectedly.
   - *Mitigation*: Ensure `execute_node` performs a clean setup check and forcefully removes existing worktrees with the same node ID before starting.

## Acceptance Criteria
- [ ] `HypothesisNode` structure and SurrealDB schema support dynamic `code_changes` maps.
- [ ] `ArborExecutor` correctly writes proposed code changes to the temp git worktree before running the test command.
- [ ] `LLMClient` uses dynamic provider configuration to prompt for code changes and score proposals.
- [ ] All HTR subcommands (`init`, `ideate`, `execute`, `backprop`, `merge`, `run`) are registered in `mythrax` CLI.
- [ ] Equivalent MCP tools are registered in the MCP server.
- [ ] End-to-end HTR run correctly performs the loop: Proposes code edits -> Runs sandboxed tests -> Evaluates results -> Merges best refinement.
- [ ] Integration tests pass cleanly.
