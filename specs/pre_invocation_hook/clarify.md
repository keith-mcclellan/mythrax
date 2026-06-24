# Clarify: Mythrax Pre-Invocation Hook

## Restated Request
Implement a system-enforced pre-invocation hook that runs automatically before every agent session/turn. The hook must check Mythrax for relevant memory and wisdom rules, automatically detect subagent status (retrieving parent-stashed STM entries and handoff details), prioritize distilled parent insights over raw episodes, inject failed sibling hypothesis nodes as HTR negative constraints, and execute passive synchronization (state journaling, vault integrity auto-healing).

## Known Facts
1. **Antigravity Harness Integration**: The active harness runs pre-invocation hooks defined in `hooks.json` in the harness's config directory (`/Users/keith/.gemini/config/hooks.json`).
2. **Current Hook Configuration**: The active `hooks.json` has a single `PreInvocation` hook that calls `verify_compliance` via the `mythrax` MCP server.
3. **Smart Handoff Protocol**: The parent agent calls `save_handoff` to write the handoff contract and copy all parent STM entries to the subagent's session in SurrealDB. The parent stashes distilled node IDs in STM under key `"distilled_context_nodes"`.
4. **SurrealDB Schema**:
   - `handoff` table matches `subagent_conversation_id` and `parent_conversation_id`.
   - `short_term_memory` table stores session-scoped key-value pairs.
   - `relates_to` is a schemaless relation (edge) table where `RELATE episode -> relates_to -> wiki_node` (and other nodes) represents links.
   - `hypothesis_node` is a schemaless table representing cognitive hypothesis tree nodes.
5. **Passive Synchronization API**:
   - `self.backend.journal_state(...)` synchronizes active git status and AST changes.
   - `self.backend.verify_vault_integrity(true)` auto-heals vault-to-DB drifts.

## Assumptions
1. **Workspace Scope Naming**: For root agents, the active project scope name will be dynamically derived from the active workspace directory's folder name (e.g. `"self-improvement-engine"` or `"mythrax"`).
2. **HTR Sibling Scope**: Sibling negative constraints will be restricted to **immediate siblings** (sharing the same `parent_id`) that have a status of `failed` or `pruned`.

## Ambiguities
- All ambiguities have been successfully resolved through the `/grill-me` design alignment phase.

## Tradeoffs
- **High-Confidence Memory Filtering**: Root agents always retrieve critical wisdom rules but only retrieve episodes/memories if they exceed a strict similarity score of `0.70` (default search threshold is `0.55`). This prevents prompt bloat while ensuring high-relevance matches are still caught automatically. A pinned instruction is injected if no high-confidence memories are found.
- **Distilled Memory Cards**: Raw episodes are never dumped fully into prompt contexts. Instead, they are rendered as compact cards with standardised footnotes prompting the agent on how to run a follow-up hydration query (`get_memory_nodes`), preserving token space.

## Blocking Questions
- None. The plan is approved and ready for execution.
