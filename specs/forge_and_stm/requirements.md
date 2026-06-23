# Requirements

## Problem
In multi-agent orchestration, context window size is a scarce resource. Spawning subagents using `/agent-handoff` forces either massive prompt size (to pass history/state) or complete state blindness. A token-conscious way to share structured key-value state dynamically is needed.
To improve developer ergonomics and streamline documentation, we need to consolidate the global `/agent-handoff` skill playbook into the project-scoped `mythrax` skill, deprecate the original global skill, enforce AST-driven symbolic precision in the local `.handoffs/` contract files, and ensure that ephemeral short-term memory files do not clutter the repository.
Additionally, when agents run, they need access to capability optimizations derived from high-fidelity reference materials (PDFs, manuals, books, markdown skills). Currently, the dynamic compiler only harvests skills, and there is no pipeline to ingest arbitrary reference documents (like official API books, architecture manuals) into a persistent memory that can suggest tips and refinements.

## Outcome
1.  A session-indexed Short Term Memory (STM) table in SurrealDB, with accompanying MCP tools and dual-write JSON files, allowing agents to store and retrieve active state variables with minimal token footprint.
2.  A Mythrax Forge ingestion pipeline that parses high-fidelity files (including PDFs via a script helper, plus markdown/text files), chunks them, synthesizes Wisdom Rules and Wiki Insights, and saves them to the Obsidian vault and SurrealDB under a dedicated `forge` scope/tier.

## User Value
-   **Lower Token Costs & Faster Inference**: Subagents start with a fraction of the prompt size while still retaining access to parent state variables.
-   **Persistent Capability Guidance**: Ingested libraries of books and manuals automatically guide the agent during codebase tasks without needing manual rule prompting.

## In Scope
-   Database table `short_term_memory` in SurrealDB.
-   New MCP tools:
    -   `put_short_term(session_id, key, value)`
    -   `get_short_term(session_id, key)`
    -   `clear_short_term(session_id)`
-   Dual-writing STM entries to `.handoffs/stm_<session_id>.json`.
-   Automatic deletion of `.handoffs/stm_<session_id>.json` file when `clear_short_term` is invoked.
-   A daily scheduled background cleanup routine in the daemon loop to purge completed/failed handoff files (`handoff_*.md`), STM JSON files (`stm_*.json`), and database records older than 7 days.
-   Consolidate `/agent-handoff` execution rules into the project-scoped `mythrax` skill and write a deprecation banner to the global `agent-handoff` skill.
-   Update handoff templates to mandate AST-symbolic links (e.g. `[ClassName](file:///path/to/file#L10-L20)`) for target code bodies.
-   Refactor local `mythrax` skill to follow the Lean Skill Paradigm, skeletonizing its contents and moving details to Forge-ingested subdirectories.
-   CLI and MCP command `forge_source` taking a local file path (PDF, markdown, txt) and parsing/chunking it.
-   Integrate pure-Rust `pdf-extract` dependency in Cargo.toml to extract text from PDF files.
-   A pipeline to feed these chunks to the LLM client, extracting Wisdom Rules (`tier: "forge"`) and Capability Insights (`scope: "forge"`).
-   Saving harvested forge nodes to `wisdom/forge/` and `wiki/forge/` directories in the workspace and syncing them to SurrealDB.
-   A memory conflict resolution protocol and precedence ranking logic defined in the unified `mythrax` skill.

## Out of Scope
-   Automated web scraping of books or URLs.
-   Converting unstructured images inside PDFs to text (no OCR).
-   Support for non-text formats (like audio, video, binary databases).
-   Multi-user permission boundaries for short term memory.

## Inputs
-   **STM**: `session_id` (string), `key` (string), `value` (string).
-   **Forge**: File path (string to local `.pdf`, `.md`, or `.txt`), target `scope` (string, defaults to "forge").

## Outputs
-   **STM**: JSON key-value response from database or local file.
-   **Forge**: Structured Wisdom Rules in `wisdom/forge/` and Wiki Nodes in `wiki/forge/`, successfully embedded and synced to SurrealDB.

## Constraints
-   STM entries must be exact-match only (no vector search needed).
-   SurrealDB schemas must enforce SCHEMAFULL schema design where applicable.
-   SurrealDB must use cosine similarity distance metric for HNSW embeddings.
-   All code changes must pass the local compilation and existing test suite.

## Assumptions
-   The task ID in `.handoffs/handoff_<task_id>.md` is the `session_id`.

## Risks and Edge Cases
-   **PDF Extraction Failure**: Large PDFs or scan-only PDFs might fail or return blank. We must handle errors gracefully and notify the user if text extraction yields no content.
-   **Token Limit in Ingestion**: High-fidelity sources can be millions of tokens. Chunking must be deterministic (e.g. splitting by character count or token approximation) to avoid blowing LLM context windows.
-   **Concurrency in STM**: Multiple subagents writing to the same STM session. Since SurrealDB handles transactions, it is safe, and local file dual-write will use atomic writes.

## Acceptance Criteria
-   [ ] SurrealDB schema updated with `short_term_memory` table.
-   [ ] MCP server implements `put_short_term`, `get_short_term`, and `clear_short_term`.
-   [ ] Storing short-term memory dual-writes to `.handoffs/stm_<session_id>.json`.
-   [ ] `clear_short_term` automatically deletes `.handoffs/stm_<session_id>.json` from disk.
-   [ ] Stale completed/failed handoffs (older than 7 days) are periodically cleaned up (deleting `handoff_*.md`, `stm_*.json`, and DB records) by the daemon's daily background scheduler.
-   [ ] Global `agent-handoff` skill marked deprecated and merged into the project-scoped `mythrax` skill.
-   [ ] Handoff templates and local contracts use precise AST symbols and line-anchored links.
-   [ ] CLI/MCP tool `forge_source` ingests a PDF or text file using pure-Rust parser, chunks it, and calls the LLM to extract Wisdom Rules and Insights.
-   [ ] Ingested rules are saved to `wisdom/forge/` and insights to `wiki/forge/` in the Obsidian vault and synced to SurrealDB.
-   [ ] Unified `mythrax` skill updated to require follow-up fetching (pagination) only for memories about skills or wisdom matches, and make it optional but strongly recommended for other memory retrieval.
-   [ ] Unified `mythrax` skill details the memory precedence hierarchy, instructs the agent to perform conflict resolution when contradiction is encountered, and mandates surfacing all resolved/unresolved conflicts in the implementation plan.
-   [ ] All tests compile and pass.

