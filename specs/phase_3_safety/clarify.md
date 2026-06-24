# Clarify: Phase 3 Advanced Session Context & Crash Safety (v0.9.x)

This document initiates Phase 1 (Clarification) of the spec-driven development process for **Phase 3: Advanced Session Context & Crash Safety** of Project Mythrax, covering:
1.  **3.1 Fault-Tolerant Mental Hydration & Hybrid Model Fallback**
2.  **3.2 Virtual Context Paging (Multi-Scale Context Swapping)**
3.  **3.3 Cognitive Checkpointing (Sliding-Window Delta Compaction)**

---

## Restated Request

Implement long-session memory resilience, crash-safety, and token-saving capabilities in Mythrax:
*   **Fault-Tolerant Mental Hydration & Hybrid Fallback**: Design an active session state journaler. On every tool call or turn, write the active HTR tree, task list (`task.md`), and active STM keys to a transaction log. If the local model hangs (detected via timeout) or crashes, automatically roll back dirty git changes to the last committed transaction, switch the model provider to a cloud fallback, rehydrate the active task state, and resume execution.
*   **Virtual Context Paging**: Replace raw file history in the active prompt with a lightweight **Symbol Page Map** referencing modified AST symbols (structs, classes, functions) whose raw edit histories are archived in SurrealDB. Intercept references or edits to these symbols, dynamically swapping their raw histories back into the active context for that turn.
*   **Cognitive Checkpointing**: Maintain a dual-speed memory by writing an immutable **Checkpoint Node** containing the active git diff and compiler state to SurrealDB every 10 minutes. Summarize only the transitions (deltas) between checkpoints, loading the last two checkpoints in their raw, high-fidelity entirety and older history as compressed summaries.

---

## Known Facts

### 1. Codebase Architecture
*   **Write-Ahead Log (WAL)**: Exists in `mythrax-core/src/wal.rs` with `log_intent` and `log_commit` for actions like `"save_episode"`.
*   **Database Backend**: `SurrealBackend` handles all node saves (episodes, wisdom, wiki nodes, handoffs, STM).
*   **Background Daemons**: `mythrax-core/src/main.rs` runs tokio background tasks for daily cleanup and dreaming/compaction runs.
*   **Arbor HTR Executor**: Exists in `mythrax-core/src/cognitive/executor.rs` and has access to git commands and test execution.

### 2. Available Hardware/Model Constraints
*   **Local Coder Agent**: Operates on `mlx-community/Qwen3.6-35B-A3B-4bit` under `mcp-openai` at port 8080.
*   **Cloud Fallback**: Swaps to a cloud model (Gemini or OpenAI) via the MCP configuration when triggered.

---

## Assumptions

1.  **Continuous Journaling**: We can capture the active state (task checklist, HTR tree state, and STM keys) and write it to the WAL on every state-changing tool call (e.g. `save_episode`, `put_short_term`, or a new `journal_state` tool).
2.  **Git Safety Boundary**: We can protect the codebase during a crash by committing or checkpointing the worktree to a temporary git branch or git stash, allowing us to safely rollback to the last known-good state on recovery.
3.  **Symbol Mapping**: We can extract symbols (classes, functions, schemas) from code diffs using a simple regex-based AST parser (for Rust, TS, Python) without requiring a full compiler frontend.
4.  **10-Minute Checkpoint Daemon**: A background thread can run a timer loop every 10 minutes to run `git diff` and `cargo check` (or language equivalent), writing the output to a `CheckpointNode` in SurrealDB.

---

## Ambiguities

1.  **Provider Swap Mechanism**: How does the system swap the active model provider from local to cloud?
    *   *Resolution*: The MCP server will expose a `swap_provider` tool, and the CLI/recovery command will update the local `config.json` or environment variables that govern the active model client, ensuring the next tool call or prompt resumes on the new provider.
2.  **Crash Recovery Trigger**: Who detects the crash or hang and runs the recovery?
    *   *Resolution*: The agent's harness (client side) or the HTR executor (server side) will monitor tool call timeouts (e.g. if the local LLM server fails to respond within 45 seconds). If a timeout occurs, it invokes the `mythrax recover` routine.
3.  **Active Page Swapping**: How does the system know when the agent is referencing a symbol in the page map?
    *   *Resolution*: When the agent calls a file viewing or search tool targeting a symbol in the page map, the server intercepts the call, retrieves the archived symbol history from SurrealDB, and injects it into the returned tool response.

---

## Tradeoffs

*   **Continuous Checkpoint Cost**: Running a background git diff and compile check every 10 minutes consumes minor CPU resources. This is acceptable because it runs entirely in the background and is extremely lightweight, while preventing hours of lost work during local model crashes.
*   **Regex AST Parsing vs. Precise Compiler Parsing**: Precise AST parsing requires heavy language-specific compiler dependencies (e.g. `syn` for Rust). Using fast, regex-based symbol extraction is lightweight, extremely fast, and highly portable across Rust, TypeScript, and Python, satisfying the Karpathy simplicity principle.

---

## Blocking Questions

*   **None**. The architectural boundaries and design objectives are aligned. We are ready to proceed to Phase 2 (Requirements) of the spec-driven development workflow.
