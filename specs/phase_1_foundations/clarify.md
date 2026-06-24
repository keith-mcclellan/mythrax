# Clarify: Phase 1 Foundations (v0.9.x)

This document initiates Phase 1 (Clarification) of the spec-driven development process for the **Phase 1 Foundations** of Project Mythrax, covering:
1.  **1.1 Zero-Config Automatic Scope Switching**
2.  **1.2 Temporal Trajectory Graphing & Sequential Replay**
3.  **1.3 Zero-Friction Automatic Failure Diagnostics**

---

## Restated Request

Implement the foundational retrieval, graphing, and diagnostic features of the Mythrax 0.9.x roadmap:
*   **Auto-Scope Switching**: Automatically detect the active project workspace context from the environment (working directories, active editor paths, git branches) and partition SurrealDB semantic search/wisdom queries to the current project scope and "general" scope without manual configuration.
*   **Temporal Trajectories**: Extend the database schema and save/retrieval pipelines to link successive episodes chronologically using first-class graph relations (`followed_by`, `superseded_by`), allowing the agent to reconstruct and replay the exact step-by-step history of past tasks and debug paths.
*   **Auto-Failure Diagnostics**: Build a low-latency, CPU-bound signature matcher that intercepts terminal command and HTR test execution failures, extracts error signatures (e.g. compilation/test codes), queries SurrealDB for past successful resolutions, and automatically appends the causal explanation and remedy directly to the error logs.

---

## Known Facts

### 1. Codebase Architecture
*   **SurrealDB Schema**: Exists in `mythrax-core/src/db/schema.rs` and initializes during database creation (`INIT_SCHEMA`).
*   **Database Backend**: `SurrealBackend` in `mythrax-core/src/db/backend.rs` handles queries, vector index definitions (HNSW), search routines, and transaction logic.
*   **MCP Server**: Exists in `mythrax-core/src/mcp.rs`, implementing a JSON-RPC 2.0 loop that maps incoming method calls to internal Rust backend methods.
*   **Workspace Root**: During MCP initialization, `McpServer::handle_request` sets the environment variable `MYTHRAX_WORKSPACE_ROOT` to the absolute path of the workspace.
*   **Arbor HTR Executor**: Exists in `mythrax-core/src/cognitive/executor.rs` and runs shell commands in a git worktree via `Command::new("sh")`.
*   **Existing Compaction**: Contains character-truncation and wisdom-rule formatting inside `SurrealBackend::compact_search_result`.

### 2. Available Database Tables
*   `episode`, `entity`, `wiki_node`, `wisdom`, `hypothesis_node`, `handoff`, `short_term_memory`, `metrics`, `relates_to`, `mentions`.

---

## Assumptions

1.  **Scope Sensing**: We can determine the active project scope by checking:
    *   The `MYTHRAX_WORKSPACE_ROOT` environment variable.
    *   The current working directory of the process.
    *   Mapping the leaf directory name (e.g. `/Users/keith/Documents/smwl` $\rightarrow$ `smwl`) to the database `scope` field.
2.  **Temporal Session Tracking**: Consecutive episodes within a session can be linked by tracking the `last_episode_id` in the Short-Term Memory (STM) table under a special key (e.g. `_last_episode_id`) for that `session_id`.
3.  **Local Match Performance**: To satisfy the $<5\text{ms}$ CPU execution constraint for error diagnostics, signature matching must use compiled regular expressions and target a quick, low-limit HNSW vector similarity search on the `wisdom` and `episode` tables.
4.  **Auto-Interception Boundary**:
    *   *For HTR*: Intercepted directly inside `ArborExecutor::execute` when `status.success()` is false.
    *   *For Agent Workflows*: Expose a new MCP tool `diagnose_failure` that the agent is instructed to call automatically when a manual command fails.

---

## Ambiguities

1.  **Active Scope Fallback**: If the active directory is not within a recognized project path, does the scope default to `"general"`?
    *   *Resolution*: Yes. If no specific project directory is detected, the search runs with `scope: Option::None` (which matches only `"general"` or queries across all scopes if explicitly requested).
2.  **Session ID Presence**: In standard `save_episode` calls, is the `session_id` always available to link the temporal chain?
    *   *Resolution*: We will update the `save_episode` MCP tool schema to accept an optional `session_id`. If provided, we look up `_last_episode_id` in STM, create the `followed_by` relation edge, and update the STM key with the new episode ID.

---

## Tradeoffs

*   **Heuristic Regex vs. Semantic LLM Diagnostics**: A semantic LLM call to classify error outputs would be slow ($>1\text{s}$) and expensive. Using compiled regex patterns for standard compilers (Rust, TypeScript, Python, etc.) combined with local HNSW vector search delivers the diagnostic results in milliseconds completely offline, preserving the Karpathy simplicity principle.
*   **Decoupled Worktrees**: Keeping HTR's internal worktree state decoupled from the main workspace crash recovery ensures that parallel execution or branch testing in Arbor doesn't interfere with the parent orchestrator's state.

---

## Blocking Questions

*   **None**. All requirements and architectural boundaries have been aligned during our strategic and `/grill-me` sessions. We are ready to proceed to Phase 2 (Requirements) of the spec-driven development workflow.
