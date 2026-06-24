# Requirements: Mythrax Pre-Invocation Hook

## Problem
Currently, agents (especially subagents) boot up without automatic access to their stashed memories, handoff documents, and relevant workspace-specific rules. This leads to a bootstrapping problem: agents must guess what context nodes to hydrate or search, wasting API tokens and LLM turns, or running in isolation with incorrect assumptions and repeating past mistakes.

## Outcome
A unified, system-enforced pre-invocation hook that automatically synchronizes active workspace state, detects subagent status, directly hydrates parent-stashed context, retrieves highly relevant project wisdom/memories, and injects active HTR sibling negative constraints into the agent's context block before the first turn begins.

## User Value
- **Zero-Eager-Prompting**: Subagents are fully aligned on their assignment and stashed context from Turn 1 without manual prompt decoration.
- **Token Efficiency**: Prevents context window bloat by prioritising distilled insights over raw transcripts and utilizing high-confidence filtering.
- **Safety Guardrails**: Automatically enforces project-scoped rules and prevents the agent from re-exploring failed paths (negative constraints).
- **Consistent Execution**: Guarantees that the agent's database matches manual filesystem edits through auto-healing synchronization.

## In Scope
- Define a new MCP tool `pre_invocation_hook` under the `mythrax` server.
- Automatically trigger `journal_state` and `verify_vault_integrity(fix=true)` at the start of the hook.
- Query SurrealDB's `handoff` table to check if the active `session_id` is a subagent.
- **Subagent Path**:
  - Retrieve handoff metadata and stashed STM entries.
  - If STM key `"distilled_context_nodes"` is present, directly hydrate those specific nodes (WikiNodes, WisdomRules, HypothesisNodes) and **bypass semantic search**.
  - If absent, perform fallback semantic search over `wisdom` and `memories` using the handoff summary.
- **Root Agent Path**:
  - Determine active scope dynamically from the workspace folder name.
  - Always search and retrieve relevant `wisdom` rules (similarity threshold `0.55`).
  - Search `memories` (episodes) using a high-confidence threshold of `0.70`. If none meet the threshold, inject a pinned deep-search instruction.
- **HTR Context & Sibling Constraints**:
  - If a hypothesis node is resolved (either explicitly stashed or most recent in the scope), format it and query for immediate sibling nodes that are `failed` or `pruned`.
  - Inject these siblings' hypotheses and critic insights as negative constraints.
- **Distilled Memory cards**:
  - Distilled nodes (WikiNodes, WisdomRules, HypothesisNodes) are fully rendered.
  - Raw episodes are rendered as compact cards with a standardised footnote instructing the agent on how to run a follow-up hydration query (`get_memory_nodes`).
  - **Abstraction Priority**: If a raw episode is linked to a distilled parent compaction/insight via the `relates_to` relation table, inject the distilled parent insight instead of the episode card.
- Configure `hooks.json` to register the new tool in the `PreInvocation` sequence.
- Write unit and integration tests covering all these paths.

## Out of Scope
- Implementing manual HTR tree traversal tools.
- Modifying client-side prompt parsing logic in the harness.

## Inputs
- `session_id` (optional string): The active agent conversation/session ID.
- `query` (optional string): The user's active prompt.
- `workspace_path` (optional string): The workspace directory path (defaults to `.` if absent).

## Outputs
- A structured markdown text report containing the agent status (Root vs. Subagent), stashed memories, hydrated context nodes, retrieved wisdom rules, high-confidence memories, active HTR context, and sibling negative constraints.

## Constraints
- **Zero Raw Episodes**: Full raw conversation transcripts must never be dumped into the hook's output.
- **No Hardcoded Paths**: All hooks and MCP configurations must resolve executable paths dynamically via `std::env::current_exe()`.
- **Low Latency**: Hook execution must complete within 2 seconds.

## Risks and Edge Cases
- **No Active DB/Daemon**: The hook must handle database connection failures gracefully and not crash the agent's startup.
- **Missing Scope Directory**: If the workspace path is empty or invalid, default to the scope name `"general"`.
- **Mismatched Node IDs**: If `"distilled_context_nodes"` contains malformed or deleted record IDs, the hook must skip them gracefully without throwing errors.

## Acceptance Criteria
- [ ] **AC-1 (State Sync)**: Pre-invocation hook successfully executes `journal_state` and `verify_vault_integrity` before retrieving memory.
- [ ] **AC-2 (Subagent Handoff)**: If the session ID matches a subagent, the hook retrieves and formats all stashed STM key-values and handoff metadata.
- [ ] **AC-3 (Direct Hydration)**: If `"distilled_context_nodes"` is present in STM, the hook directly hydrates those specific nodes and bypasses semantic search.
- [ ] **AC-4 (Root Agent Dynamic Retrieval)**: If the session is a root agent, the hook retrieves wisdom rules at similarity `0.55` and episodes/memories only at similarity `0.70`.
- [ ] **AC-5 (Pinned Deep-Search Reminder)**: If no high-confidence memories (>0.70) are found for a root agent, a pinned reminder instruction is injected.
- [ ] **AC-6 (HTR Sibling Negative Constraints)**: If an active hypothesis node is resolved, the hook retrieves and injects all immediate sibling nodes that have status `failed` or `pruned` along with their critic insights.
- [ ] **AC-7 (Abstraction Priority)**: If a retrieved episode has an upward `relates_to` link to a distilled parent insight/compaction, the parent insight is injected instead of the episode card.
- [ ] **AC-8 (Distilled Memory Cards)**: Raw episodes are rendered only as compact metadata cards containing their record ID, title, scope, and a standardized hydration footnote.
- [ ] **AC-9 (Harness Integration)**: Running `mythrax config antigravity` successfully appends both the compliance hook and the new `pre_invocation_hook` to `hooks.json`.
