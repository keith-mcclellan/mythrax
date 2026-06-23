# Requirements: Phase 5 (v0.6.0) — Daemon Autonomy & Node Compression

## Problem
Currently:
1. Handoff and short-term memory (STM) cleanup is only executed once every 24 hours in the daily scheduler loop, or manually during tests. Stale STM database records and `stm_*.json` files that are orphaned (not linked to an active handoff) can accumulate indefinitely.
2. The retrieval pipeline in `SurrealBackend::search` employs an all-or-nothing token budgeting system. If a candidate search result exceeds the remaining token budget, it is completely omitted, even if it contains highly relevant context that could be compressed or truncated.
3. The background daemon can only be launched via `mythrax daemon start`, which writes a PID file but lacks a standard `run` foreground execution mode suitable for container orchestration or direct interactive logging.
4. Semantic memory retrieval queries raw episode transcripts by default. These raw logs contain highly verbose developer interactions and transcript logs, which leads to immediate context bloat when multiple episodes are loaded into an agent prompt.

## Outcome
1. Continuous pruning of stale handoff records, stale STM files, and orphaned database STM records (older than 3 days) will be executed on daemon startup, and inside both dreaming and compactor loops.
2. Retried search results that exceed the remaining token budget will be dynamically compressed:
   - Wisdom rules will have their causal explanation (`**Why**:`) field stripped.
   - Episodes and insights will be compacted to their first paragraph or truncated deterministically using binary search to fit the exact available budget.
   - The truncation notice `... [Truncated (Inner-Node Compaction)]` will only be appended if content was actually removed from the candidate node.
3. A new `mythrax daemon run` CLI subcommand will run the daemon in the foreground, enabling direct terminal logging, standard process orchestration, and PID file cleanup on SIGINT/Ctrl+C.
4. By default, semantic search will exclude the `episode` table and prevent graph traversal into `episode` nodes. An optional `include_episodes` flag/parameter will be provided in the DB API, Axum endpoint, MCP tool, and CLI to retrieve raw episodes when explicitly required:
   - If `include_episodes` is true, graph traversal downward to episodes will only be allowed if `allow_downward` is also true.
   - If `include_episodes` is false, graph traversal to episodes is completely blocked.

## User Value
- Clean, self-pruning Obsidian vault and SurrealDB with zero manual cleanup required.
- More context-dense agent prompts because nodes are compacted to fit the budget rather than being omitted entirely.
- Improved daemon manageability in container/foreground environments.
- Massive reduction in prompt context bloat by excluding verbose raw episodes by default.

## In Scope
1. Implementing orphaned STM record and file pruning in `SurrealBackend` and the daemon.
2. Integrating pruning into `DreamCoordinator::run_dream`, `Compactor::compact_scope`, `Compactor::compact_global`, and daemon startup.
3. Implementing deterministic inner-node compaction in `SurrealBackend::search`.
4. Adding the `run` command to the `DaemonAction` CLI interface and wiring it up in `main.rs`.
5. Modifying `StorageBackend::search` and `SurrealBackend::search` to accept `include_episodes: bool` parameter. If false, exclude the `episode` table search and exclude `episode` from relates_to/mentions target tables.
6. Adding `include_episodes` parameter to the `/v1/search` HTTP route and `search_memories` MCP tool.
7. Adding an `--episodes` flag to the `mythrax search` CLI command.

## Out of Scope
- Real-time LLM-based summarization of nodes during vector search (real-time searches must remain sub-second and deterministic).
- Modifying the markdown schema structure of wiki nodes on disk.

## Inputs
- `token_budget` limit during search query execution.
- Handoff records, STM files, and STM database table entries.
- CLI subcommand arguments for `mythrax daemon run`.
- `include_episodes` parameter in search requests.

## Outputs
- Pruned STM files (`stm_*.json`) and handoff markdown files.
- Deleted STM/handoff database records.
- Compacted `SearchResult` structs returned by `search_memories`.
- Foreground running Axum server process.
- Filtered search results excluding raw episodes by default.

## Constraints
- Pruning threshold is 3 days (`time::now() - 3d` for database records, 3 days elapsed since last modification for files).
- Token calculations must match the `count_text_tokens` method.

## Assumptions
- Handoff and STM files reside in the `.handoffs` subdirectory of the vault root.
- The `nomic-embed-text` token count method is available via the `embedder`.

## Risks and Edge Cases
- **Compaction leads to zero content**: If the remaining budget is extremely small (e.g. less than the title and a truncation notice), the node must still be omitted. We must handle this gracefully.
- **Concurrent DB writes during pruning**: The database uses RocksDB embedded mode which only allows a single process. Since Axum and background tasks run in the same process, we must ensure transactions do not block each other.
- **Tests break due to default episode exclusion**: Existing tests that expect search results to contain episodes will break if they don't explicitly pass `include_episodes: true`. We must update all search invocations in test files.

## Acceptance Criteria
- [ ] Database pruning deletes all STM records not updated for 3 days.
- [ ] File pruning deletes all `stm_*.json` files in `.handoffs/` not modified for 3 days.
- [ ] Pruning is executed on daemon startup, at the end of dreaming (`run_dream`), and during scope/global compactions.
- [ ] If a search result exceeds the remaining budget during `search`, it is compacted:
  - If a wisdom rule, its `**Why**:` field is stripped.
  - If an episode or insight, it is truncated to its first paragraph, or binary-searched to fit exactly. Suffix `... [Truncated (Inner-Node Compaction)]` is only appended if content was actually removed.
- [ ] `mythrax daemon run` starts the Axum server and background loops in the foreground, writing a PID file and cleaning it up on exit.
- [ ] Semantic search excludes `episode` table and related `episode` nodes by default (when `include_episodes` is false/omitted).
- [ ] `search_memories` MCP tool, Axum `/v1/search` endpoint, and CLI `search` command support `include_episodes` flag/parameter.
- [ ] Graph traversal downward to episodes is only allowed when both `include_episodes` and `allow_downward` are true.
- [ ] 100% test pass rate on all new and existing tests.
