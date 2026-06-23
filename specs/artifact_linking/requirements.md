# Requirements: Artifact Ingestion Linking & Local Model Stabilization

## Problem
Obsidian vault navigation, semantic search, and LLM dreaming are hindered because raw conversation episodes and their generated artifacts (plans, tasks, walkthroughs) are completely disconnected. The dreaming agent does not see the detailed plans and walkthroughs when summarizing episodes, leading to incomplete or shallow insights. Additionally, during bulk dreaming operations, the local LLM server crashes under memory pressure (KV cache allocation size) and the HTTP client fails to recover because of short retry timeouts. If we simply truncate long episodes, we lose high-signal context.

## Outcome
- Raw conversation logs are chunked into parts (max 100,000 characters) during ingestion. This prevents context loss from truncation, ensures high-quality vector embeddings within the local ONNX context limit, and keeps dreaming prompts bounded.
- All newly ingested raw episode parts contain a `## Linked Artifacts` section with wikilinks to all artifacts created during that conversation.
- All newly ingested artifacts contain a footer linking back to all generated episode parts of the parent conversation.
- The SurrealDB graph database contains directed relationships (`relates_to` edge) connecting each `episode` part node to the artifact `wiki_node` nodes.
- During LLM dreaming/compaction summary cycles, the `DreamCoordinator` pulls in the associated artifacts of each episode part to enrich the context sent to the LLM.
- Local LLM execution is stable, with smaller memory footprint (KV cache size) and self-healing network retries that wait for server recovery if a crash occurs.

## In Scope
- Implement a log-chunking utility and update `bulk_ingest_vault` for the `antigravity` harness in `mythrax-core/src/vault/ingestion.rs` to chunk logs, link files, and save `relates_to` edges.
- Add `get_related_node_ids` to `StorageBackend` trait and implement it in `SurrealBackend`.
- Modify `run_dream` in `mythrax-core/src/cognitive/synthesis.rs` to fetch related artifact wiki nodes for each episode part and include them in the LLM prompt.
- Stabilize the local LLM client in `mythrax-core/src/llm/mod.rs` by:
  - Dynamically adjusting `max_tokens` (2048 for summaries/compaction, 4096 for code ideation) to reduce KV cache size.
  - Applying a safety context window truncation at 100,000 characters for the combined prompt.
  - Upgrading `send_with_retry` to execute up to 6 attempts with a maximum 5-second backoff delay (total wait ~17s) to tolerate server restarts.

## Out of Scope
- Linking or modifying other harnesses (claude, cursor) which do not use structured conversation-level artifacts in the same folder layout.

## Acceptance Criteria
1. Conversations exceeding 100,000 characters are chunked into separate Obsidian files and SurrealDB nodes.
2. Newly ingested episode parts contain wikilinks to their artifacts.
3. Ingested artifacts contain a footer linking back to all parts of the parent episode.
4. The database contains a `relates_to` edge from each episode part to each artifact.
5. During dreaming (summarization), the prompt text sent to the LLM includes the content of the linked artifacts, capped at 100,000 characters.
6. `max_tokens` payload for local chat completions defaults to 8192.
7. Local completions pause for 5 seconds between consecutive requests to allow GPU/cache recovery.
8. HTTP client retries up to 6 times with exponential backoff on local model connection refused errors.



