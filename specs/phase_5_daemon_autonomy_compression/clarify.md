# Clarify: Phase 5 (v0.6.0) — Daemon Autonomy & Node Compression

## Restated Request
Implement the Phase 5 (v0.6.0) features of the Mythrax Cognitive Pipeline:
1. **A.1 Continuous STM & Handoff Daemon Pruning**: Automatically evict expired short-term memories and handoff records in the background daemon/compactor loops to prune expired contexts and clean up files from disk.
2. **A.2 Fine-Grained Inner-Node Compaction**: When token budgeting is active, allow the compactor/synthesizer to dynamically compress the internal fields of a node (e.g., removing causal explanations or summarizing long contents of an Episode/Insight) to fit more nodes within the budget, rather than performing all-or-nothing node truncation.
3. **Daemon CLI**: Introduce a unified long-running background command (`mythrax daemon run`) orchestrating these loops on a schedule or file-watching triggers.
4. **Episode Filtering & Exclusion (Bloat Prevention)**: Exclude raw episodes by default from semantic memory searches and graph traversals to prevent prompt context bloat. Offer a follow-up option/command to include raw episodes when explicitly requested.

## Known Facts
- **Pruning Logic**: `SurrealBackend::delete_stale_handoffs` currently queries completed/failed handoffs older than 7 days, removes their files on disk, removes their STM files (`stm_*.json`), and deletes matching SurrealDB `short_term_memory` records.
- **Background Loop**: In `main.rs`, the daemon starts a daily loop that runs `delete_stale_handoffs` every 24 hours.
- **Obsidian Vault layout**: Handoffs and STM files are placed inside `vault_root/.handoffs/`.
- **Token Budgeting**: Implemented in `SurrealBackend::search` where candidates are sorted by tier priority and kept until `cumulative_tokens > budget`. Omitted candidates are put into `omitted_ids`.
- **STM Table**: The `short_term_memory` table has `session_id`, `key`, `value`, and `updated_at` (default `time::now()`).
- **Search Tables**: By default, `SurrealBackend::search` queries `episode`, `wiki_node`, and `wisdom` tables in parallel, and includes `episode` in related nodes graph traversal.

## Assumptions
1. **Continuous Pruning Integration**: Continuous pruning should be integrated directly into:
   - The compactor loops (`compact_scope` / `compact_global`).
   - The dreaming loops (`run_dream`).
   - When starting the daemon (`mythrax daemon start` or `mythrax daemon run`).
2. **STM Table Eviction**: We should evict any SurrealDB `short_term_memory` record that has not been updated for 3 days (`updated_at < time::now() - 3d`), even if it is not associated with an explicit handoff record.
3. **Orphaned STM Files**: We should search the `.handoffs/` directory and prune any `stm_*.json` file whose modification time is older than 3 days.
4. **Heuristic Inner-Node Compaction**: Real-time search cannot use slow LLM completions. We will implement high-performance, deterministic inner-node compaction:
   - **Wisdom Rules**: Remove the `**Why**:` field.
   - **Episodes/Insights**: Attempt to keep only the first paragraph (split by `\n\n`), or binary-search character truncation to fit the remaining budget exactly, appending `\n... [Truncated (Inner-Node Compaction)]`.
5. **Daemon CLI 'run' vs 'start'**:
   - `mythrax daemon start` starts the daemon with Axum HTTP listening and PID tracking.
   - `mythrax daemon run` will perform the exact same launch operations but in the foreground of the current process, serving as a standard entry point for containers or terminal sessions.
6. **Default Search Behavior**: Semantic search will default to `include_episodes = false`. This will skip querying the `episode` table and will omit `episode` from graph traversal target lists, unless `include_episodes: true` is explicitly requested.

## Tradeoffs
- **DBSCAN & Harvester Integration with Pruning**: Pruning is database and disk-maintenance-oriented. Running it inside dreaming and compactor loops is safe, but we should make sure it runs asynchronously or concurrently so it does not block the real-time file watcher.
- **Real-Time Token Truncation**: Deterministic token compaction is extremely fast, predictable, and doesn't pollute external logs or consume LLM tokens.
- **Filtering Episodes vs Recall**: Excluding episodes reduces prompt bloat significantly, but might hide historical execution logs from the agent unless they explicitly request it. Therefore, exposing the `include_episodes` parameter in MCP and a `--episodes` flag on the CLI search command is crucial.

## Blocking Questions
- None. The requirements are clear, and the design proposals are direct.
