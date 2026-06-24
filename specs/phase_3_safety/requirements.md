# Requirements: Phase 3 Advanced Session Context & Crash Safety (v0.9.x)

This document defines the requirements for Phase 3 of the Mythrax 0.9.x features.

---

## Problem

1.  **Vulnerability to Local Crashes**: Local model execution (running on CPU/GPU) is susceptible to hangs, token-length bottlenecks, and out-of-memory (OOM) crashes. When a crash occurs during a complex coding turn, it leaves the git directory in a half-modified, dirty state and completely erases the agent's active cognitive state (task lists, HTR trees, short-term memory keys), requiring a painful manual reconstruction.
2.  **Prompt Bloat from Large Files**: Editing and viewing multiple source files over a long session fills the agent's context window with thousands of lines of raw code history. This degrades the reasoning capabilities of local models, increases latency, and quickly exceeds token budgets.
3.  **Compaction Quality Tradeoffs**: Standard prompt compaction runs at a single speed—either summarizing everything (losing recent compiler error details and exact code lines) or keeping everything (bloating the prompt). There is no "dual-speed" mechanism that preserves high-fidelity recent milestones while compressing older transitions.

---

## Outcome

*   **Continuous Crash Safety**: The agent's cognitive state is continuously journaled. If the local model crashes, the system rolls back dirty git files, swaps the model provider to a cloud fallback, and rehydrates the exact task list and STM state to resume execution seamlessly.
*   **Symbol Paging**: Raw file history in the prompt is replaced by a lightweight Symbol Page Map. When the agent references or edits a paged symbol, the system dynamically swaps its raw history back into the active context on-demand.
*   **Dual-Speed Compaction**: The system captures immutable checkpoints of the git diff and compiler state every 10 minutes. Compaction summarizes only the transitions between checkpoints, keeping the last two checkpoints in their raw, high-fidelity entirety.

---

## User Value

*   **Bulletproof Resilience**: Zero lost progress. If the local GPU runs out of memory or hangs, the system recovers instantly on the cloud fallback without manual intervention.
*   **80% Prompt Token Savings**: Context footprint is kept tiny by replacing massive raw file histories with a lightweight page map, keeping the agent fast and highly accurate.
*   **Lossless Recent History**: The agent retains exact, high-fidelity compiler states and recent code lines, while older history is cleanly compressed.

---

## In Scope

*   **Fault-Tolerant Mental Hydration & Hybrid Fallback**:
    *   Implement state journaling that writes the active task checklist (`task.md`), HTR tree state, and STM keys to the WAL on every state-changing tool call.
    *   Expose a CLI and recovery command `mythrax recover --session <session_id>` that:
        1. Resets the git worktree to the last known-good transaction commit.
        2. Swaps the active provider configuration to the cloud fallback.
        3. Rehydrates the task list and active STM keys, printing the recovery state.
*   **Virtual Context Paging**:
    *   Create a regex-based AST symbol extractor for Rust, TypeScript, and Python to parse classes, structs, and functions from code diffs.
    *   Archive raw symbol edit histories in SurrealDB and replace them in the prompt with a Symbol Page Map.
    *   Implement an interception hook in the search and file retrieval engines to dynamically swap symbol histories back into the context on-demand.
*   **Cognitive Checkpointing**:
    *   Implement a background checkpoint daemon that runs every 10 minutes to save a `CheckpointNode` containing the active git diff and compiler results to SurrealDB.
    *   Implement delta compaction that summarizes only transitions between checkpoints, loading the last two checkpoints raw.

---

## Out of Scope

*   Recovering from complete OS or hardware crashes (e.g. computer power failure).
*   Automatic cloud fallback for third-party tools that are not managed by the Mythrax MCP server.
*   Integrating external language server protocol (LSP) servers for symbol extraction.

---

## Inputs

*   **For Recovery**: Active WAL entries, current git status, and the model configuration file (`config.json`).
*   **For Paging**: Raw code diffs, file paths, and agent search/view queries.
*   **For Checkpoints**: Git diff output and local compiler execution results (e.g. `cargo check` output).

---

## Outputs

*   **For Recovery**: A clean, rolled-back git directory and a rehydrated active task state running on the cloud fallback provider.
*   **For Paging**: A lightweight Symbol Page Map in the prompt, and dynamically hydrated symbol histories in tool responses.
*   **For Checkpoints**: Saved `CheckpointNode` records in SurrealDB and dual-speed compacted prompts.

---

## Constraints

*   **Recovery Reliability**: Recovery must be fully offline and local-first, succeeding even if internet connectivity is intermittent (until the cloud fallback is activated).
*   **Symbol Extraction Speed**: Symbol extraction from code diffs must run in $<10\text{ms}$ on CPU.
*   **Paging Transparency**: The transition from symbol references to raw history must be completely transparent to the agent, requiring no special tool calls.

---

## Risks and Edge Cases

*   **Git Rollback Data Loss**: Rolling back to the last transaction could discard very recent unsaved edits.
    *   *Remedy*: The system stashes any dirty modifications to a temporary recovery branch (`recovery-stash-<timestamp>`) before executing the hard reset, ensuring the developer can recover stashed edits if needed.
*   **Cloud Fallback Cost**: Switching to the cloud fallback might incur API usage costs.
    *   *Remedy*: The system will log a prominent warning and prompt the user (or respect the `allow_cloud_fallback` setting) before making the first cloud API call.

---

## Acceptance Criteria

*   **[ ] AC-3.1 (State Journaling)**: Every state-changing tool call successfully journals the active task checklist, HTR node state, and STM keys to the WAL.
*   **[ ] AC-3.2 (Recovery Execution)**: Simulating a local model timeout and running `mythrax recover` successfully stashes dirty changes, resets git to the last transaction commit, swaps the active provider to the cloud fallback, and prints the rehydrated task state.
*   **[ ] AC-3.3 (Symbol Extraction & Page Map)**: Running compaction on code edits successfully extracts AST symbols (structs, functions) and archives their raw change logs, replacing them in the prompt with a lightweight `Symbol Page Map` containing file paths and page references.
*   **[ ] AC-3.4 (Dynamic Page Swapping)**: When the agent references a symbol in the page map during a search or file view, the system dynamically retrieves and swaps the raw edit history back into the context for that turn.
*   **[ ] AC-3.5 (Cognitive Checkpoints)**: The checkpointing daemon successfully saves a `CheckpointNode` containing the active git diff and `cargo check` results to SurrealDB every 10 minutes, maintaining the last 2 checkpoints raw and compacting older history.
