# Clarify - HTR Loop Generalization & CLI/MCP Integration

## Restated Request
Generalize the Hypothesis-Tree Refinement (Arbor) loop from its current mock prime-checker implementation to a generic, generalist research framework. Wire it into the unified Rust core so it can run dynamic cognitive research iterations and be accessed both via end-to-end and low-level CLI commands and MCP tools.

## Known Facts
- The codebase contains the HTR loop implementation as library code in `cognitive/arbor.rs`, `cognitive/executor.rs`, and `cognitive/critic.rs`.
- Currently, this code is hardcoded to a Python prime sieve example and is only tested inside `tests/test_arbor_htr_loop_lifecycle.rs`.
- `LLMClient` in `src/llm/mod.rs` already implements the `ArborLlmClient` trait, but its prompts are hardcoded to the prime sieve example.
- The `mcp.rs` and `cli.rs` do not expose HTR/Arbor capabilities.

## Assumptions
- The test command to run the evaluation is an arbitrary shell command (e.g. `pytest`, `cargo test`, `python3 test.py`).
- The research is focused on a specific set of codebase files, defined as "target files".
- Diffs or complete code replacements for each node are stored directly in `HypothesisNode` under a new field.
- The default branch in the git repository is `main` (or whatever the active HEAD branch is).

## Ambiguities
- *Resolved*: The CLI/MCP will support both end-to-end run commands and step-by-step low-level commands.
- *Resolved*: Code changes will be stored as a file path to file content map in `HypothesisNode`.
- *Resolved*: Code base context is supplied via a list of "target files" whose content is sent to the LLM during ideation.

## Tradeoffs
- **File Map vs. Unified Diffs**: Storing full file content replacement maps in `HypothesisNode` is chosen over unified diff patches because it is simpler to write programmatically and robust against patching merge conflicts in the worktree. However, it consumes slightly more database space.
- **Git Branch Merging vs. Direct File Overwrite**: Direct file writing in the main repository on merge avoids merge conflicts and commits clean, pre-tested states. However, it doesn't preserve git branch history (which is fine since the temporary branch is deleted anyway).

## Blocking Questions
None. All design choices have been aligned on during the interview.
