# Requirements: Phase 1 Foundations (v0.9.x)

This document defines the requirements for Phase 1 of the Mythrax 0.9.x features.

---

## Problem

1.  **Friction in Scoping**: Developers and agents frequently work across multiple projects in a single environment. Manually configuring and passing `--scope` or filtering queries is error-prone, resulting in cross-project context contamination (e.g. leaking mobile web styles into database packages).
2.  **Lack of Chronological Narrative**: Vector searches retrieve isolated, fragmented matching text blocks. The sequential, step-by-step history of how a complex compilation error or debugging task was resolved is lost, forcing the agent to repeat trial-and-error steps.
3.  **Prompt Bloat from Stderr**: Dumping raw compiler outputs, stack traces, and test logs directly into the agent's prompt consumes massive token counts and degrades model reasoning.

---

## Outcome

*   **Frictionless Scoping**: Scopes are automatically detected from the active file path, current directory, or git context. Vector queries automatically partition search bounds to the active project and general rules.
*   **Procedural Memory Chains**: Successive episodes are linked using native SurrealDB graph relationships (`followed_by`, `superseded_by`). Searches can retrieve full chronological execution pathways on-demand.
*   **Zero-Friction Diagnostics**: Command and compiler failures are intercepted and matched against past resolutions locally. The agent receives a clean, concise remedy instead of a raw wall of stderr logs, reducing token consumption and speed-to-fix.

---

## User Value

*   **Zero Configuration**: The memory engine adapts invisibly to where the developer is working.
*   **Faster Debug Loops**: The agent instantly remembers how it solved a similar compiler error or configuration roadblock previously, bypassing repeating debug trials.
*   **Lower Context Costs**: Replaces huge raw error logs with tiny, highly focused diagnostic footnotes.

---

## In Scope

*   **Auto-Scoping**:
    *   Extract the active project scope name dynamically from the environment.
    *   Update `search_memories` and `search_wisdom` to automatically apply the detected scope when no explicit scope is passed.
*   **Temporal Graphing**:
    *   Extend SurrealDB schema to support `followed_by` and `superseded_by` edges.
    *   Track `_last_episode_id` in Short-Term Memory (STM) per session.
    *   Add graph traversal logic to `search_memories` to fetch adjacent temporal nodes when `deep_insight: true` is set.
*   **Failure Diagnostics**:
    *   Expose `diagnose_failure` tool in the MCP server.
    *   Integrate error signature matching in `ArborExecutor::execute` when a test command fails.
    *   Use fast CPU-based regex compiled patterns and low-limit HNSW vector queries to resolve diagnostic remedies in $<5\text{ms}$.

---

## Out of Scope

*   Automatic git commits or branches created for the user outside of isolated HTR worktrees.
*   Multi-hop graph visualization in a web UI (deferred to Phase 2/Obsidian).
*   Automatic cloud fallback on OOMs (deferred to Phase 3: Crash Recovery).

---

## Inputs

*   **For Auto-Scoping**: Process current working directory (`cwd`), active file path from IDE, or `MYTHRAX_WORKSPACE_ROOT` environment variable.
*   **For Temporal Graphing**: Optional `session_id` in `save_episode` tool parameter list.
*   **For Failure Diagnostics**: `stderr`, `stdout`, `exit_code`, and `command` strings passed to the `diagnose_failure` tool or intercepted inside the HTR execution run.

---

## Outputs

*   **Auto-Scoping**: Filtered search outputs matching only `$active_scope` and `"general"`.
*   **Temporal Graphing**: Search response containing matching episodes populated with their adjacent `related_nodes` temporal chains.
*   **Failure Diagnostics**: Decorated error block containing:
    *   `Causal Explanation` (Why the command failed).
    *   `Prescribed Remedy` (Surgical fix or command to run instead).

---

## Constraints

*   **Diagnostic Latency**: Must execute in $<5\text{ms}$ on CPU to prevent locking up active terminal commands.
*   **Zero-Config Requirement**: No manual scope configuration files or environment setups can be required of the developer.
*   **Database Compatibility**: Must use SurrealDB 3.1+ HNSW index structures and standard SurrealQL relationship syntax.

---

## Assumptions

*   The environment variable `MYTHRAX_WORKSPACE_ROOT` is successfully set by the agent harness during the MCP `initialize` call.
*   Compilation and terminal failures contain standard, parseable signatures (e.g. `E0432`, `TS2322`, `401 Unauthorized`, `lock acquisition failure`) that can be matched via regex.

---

## Risks and Edge Cases

*   **Scope Mismatch**: Working in a deep sub-folder within a project might resolve the scope to the sub-folder name instead of the project root.
    *   *Remedy*: Traverse up the path to find the nearest parent containing a `.git` folder or a project marker (like `Cargo.toml`, `package.json`, or `.agents/`).
*   **Missing Session ID**: If an episode is saved without a `session_id`, the temporal link cannot be formed.
    *   *Remedy*: Fall back to a global default session or skip temporal linking for that specific episode.

---

## Acceptance Criteria

*   **[ ] AC-1.1**: When a query is run from `/Users/keith/Documents/self-improvement-engine/mythrax-core/`, the scope is dynamically resolved as `"mythrax"` and search queries filter out results from other scopes (e.g., `"smwl"`).
*   **[ ] AC-1.2**: Sequential calls to `save_episode` passing `session_id: "test-sess-123"` successfully create `followed_by` relation edges linking the records in SurrealDB.
*   **[ ] AC-1.3**: Querying `search_memories` with `deep_insight: true` on a temporally linked episode returns the matched episode along with its preceding and succeeding adjacent episode details in the `related_nodes` array.
*   **[ ] AC-1.4**: Calling `diagnose_failure` with an error signature matches a corresponding `WisdomRule` or resolved `Episode` and returns the remedy in $<5\text{ms}$ on CPU.
*   **[ ] AC-1.5**: HTR test failures automatically append the causal explanation and remedy to the execution log, verifying that the HTR Critic receives the diagnostic solution without manual intervention.
