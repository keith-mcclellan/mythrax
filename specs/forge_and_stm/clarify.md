# Clarify

## Restated Request
Design and implement two additional capabilities for Project Mythrax:
1.  **Short Term Memory (STM)**: A lightweight, token-conscious session-based memory context sharing system between parent agents and subagents, enhancing the `/agent-handoff` protocol.
2.  **Mythrax Forge**: An ingestion pipeline that processes high-fidelity source materials (PDFs, books, markdown skills) into structured Wisdom Rules and Wiki Insights, creating a persistent capability memory that provides refinements and hints to agents on when to invoke specific skills.

## Known Facts
-   The current `/agent-handoff` protocol relies on file-based contracts (`.handoffs/handoff_<task_id>.md` and `.handoffs/handoff_<task_id>_return.md`).
-   SurrealDB is used as the unified storage backend, containing tables for `episode`, `wisdom`, `wiki_node`, and `handoff`.
-   The existing `harvest_skills` system scans skill playbooks and uses the local/cloud LLM to synthesize Wisdom Rules.
-   The ONNX embedder generates 768-dimensional float vectors using cosine distance.

## Assumptions
-   The `task_id` (or `parent_conversation_id`) of the handoff contract serves as the `session_id` for Short Term Memory.
-   Short Term Memory does not need vector search or embedding generation; it is a structured key-value state store.
-   The Mythrax Forge is invoked via a CLI command and/or MCP tool.
-   High-fidelity PDF files can be parsed directly using the pure-Rust `pdf-extract` crate, then fed to the chunking/summarization pipeline.
-   Forge-ingested memories will be categorized with `tier: "forge"` or `scope: "forge"` to prevent mixing with dynamic developer episodes, while allowing standard search tools (`search_wisdom`, `search_memories`) to retrieve them.
-   The existing `agent-handoff` global skill will be consolidated directly into the local `mythrax` skill, and deprecated at its global location.
-   Handoff contracts written to `.handoffs/` must identify targets using precise AST-symbolic links (e.g. `[ClassName](file:///path/to/file#L10-L20)`) to maintain Karpathy-based surgical changes.

## Ambiguities
-   **STM Cleanup & Background Scheduled Handoff Cleanup**: When does STM expire? We assume it should live for the duration of the task session and can be cleared explicitly using a `clear_short_term` endpoint. To prevent disk clutter, `clear_short_term` will delete the SurrealDB records *and* delete the ephemeral `.handoffs/stm_<session_id>.json` file from disk. Additionally, the daily background daemon scheduler will periodically identify completed/failed handoff contracts older than 7 days, delete their respective markdown files (`handoff_*.md`) and STM JSON files from the filesystem, and delete their records from SurrealDB.
-   **STM File Dual-Write**: Should STM dual-write to a local file? Yes, writing to `.handoffs/stm_<session_id>.json` ensures transparency, version control, and ease of inspection.
-   **Forge Chunking Strategy**: How do we handle extremely large files? We will chunk high-fidelity sources into ~2000-token blocks, generate embeddings/summaries for each block, and ask the LLM to extract Wisdom Rules and Capability Insights per chunk.
-   **Forge PDF Parser**: We will use the pure-Rust `pdf-extract` crate. This removes external runtime dependencies (Python interpreter, pip packages, virtual environments) and provides a unified, compiled binary solution.
-   **Lean Skill Paradigm**: To keep skills token-conscious, playbooks are stripped of verbose instructions, examples, and edge-cases. All verbose materials are stored in `examples/` and `references/` subdirectories and ingested via the Forge into SurrealDB. The `SKILL.md` file acts solely as a lightweight declaration, reducing active prompt context sizes.
-   **Pagination-Aware Retrieval**: Follow-up fetching (pagination) is **only required** when retrieving memories about skills or wisdom matches. For other memory types (such as developer episodes or general logs), follow-up fetching is **optional but strongly recommended** to avoid prompt bloating while ensuring critical context is not missed. The unified `mythrax` skill will explicitly instruct the agent to check the `PAGINATION NOTICE` and execute follow-up queries with adjusted `offset` and `limit` parameters when needed.
-   **Memory & Instruction Conflict Resolution**: When multiple skills or memories provide conflicting instructions, the agent needs a deterministic precedence hierarchy to resolve contradictions:
    1.  *User Overrides*: Direct instructions in the user's prompt or `AGENT.md` (workspace rules) always take absolute precedence.
    2.  *Active Workspace Skills*: Local `.agents/skills/<name>/SKILL.md` overrides global or ingested rules.
    3.  *Developer/Empirical Episodes*: Dynamic memories (`tier: "developer"`, `scope: "general"`) documenting actual developer fixes, test results, or codebase constraints override static reference manuals.
    4.  *Ingested Forge Wisdom*: Static guidelines harvested from books or manuals (`tier: "forge"`).
    5.  *Global / Fallback Skills*: Defaults outside the active workspace.
    Within the same tier (e.g. two conflicting developer episodes), the more recent memory (higher timestamp/updated_at) takes precedence. All identified conflicts or ambiguities (even if successfully resolved) must be documented in the implementation plan under 'User Review Required' showing the resolution decision. If a conflict cannot be resolved, the agent must prompt the user directly for clarification.

## Tradeoffs
-   **SurrealDB vs. Local File for STM**: SurrealDB allows fast, atomic read/writes via MCP tools from any subagent container, while local files allow git tracking and inspection. Dual-write balances both, and our automated deletion of the ephemeral JSON files removes the downside of local git pollution and file build-up.
-   **Granular vs. Consolidated Forge Rules**: Generating a wisdom rule for every capability pattern can lead to rule bloat. We will instruct the LLM to consolidate rules and extract only high-fidelity prescriptive patterns (avoid, why, remedy).

## Blocking Questions
None. The design constraints are clear.
