# Specification: Advanced Agentic Memory Systems Integration

This document defines the formal engineering specification for integrating next-generation agentic memory paradigms into the Mythrax platform. It establishes the technical requirements, structural designs, testing methodologies, and implementation tasks required to build a highly performant, self-regulating, and cognitively consistent memory substrate.

---

## 1. Clarify

### Restated Request
Evolve the Mythrax memory architecture by integrating nine advanced cognitive memory paradigms, maintaining the existing client-server daemon topology, ONNX embedding centralization, and SurrealDB graph engine.

### Core Paradigms & Self-Corrected Decisions (v3)
1.  **CoALA Integration**: Formalize the agent's decision cycle (Plan $\rightarrow$ Retrieve $\rightarrow$ Decide $\rightarrow$ Act $\rightarrow$ Learn) using a coordinator-executor topology.
2.  **MemoryOS Paging (Virtual Skeletons & Paging-Aware Editing)**: 
    *   *Disk Integrity*: Physical files on disk are never modified. The daemon generates **virtual skeletons** in memory during file reads (like `view_file`), keeping the codebase fully compiling.
    *   *AST-Patch Paradox Resolution*: The MCP editing tools (`replace_file_content`, `multi_replace_file_content`) are **paging-aware**. When they receive an editing patch containing placeholders, they fetch the full-text bodies from `symbol_archive` in SurrealDB, reconstruct the unpaged target block in memory, and safely apply the edit to the physical file on disk.
3.  **A-MEM Engine**: Dynamically decaying and reinforcing episodic memories in the database. 
    *   *Sync Loop Resolution*: Transient metadata updates (like `utility` decay or `last_retrieved_at` updates) are stored purely in the database and **never written back to disk**, preventing infinite sync loops.
    *   *Split-Brain Mitigation*: To prevent a human or watcher sync from overwriting and resetting dynamic cognitive metadata, `sync_file_to_db` in `watcher.rs` uses `UPDATE ... MERGE` queries, updating only structural content (title, content, scope) while preserving existing cognitive metadata (utility, last_retrieved_at) in SurrealDB.
4.  **POMDP Belief State**: Injecting a persistent, self-correcting `BeliefState` node into the pre-invocation hook, updated by the daemon after every tool execution.
5.  **Think-in-Memory (TiM)**: Extracting abstract thought nodes from execution traces and saving them to the vault under `wiki/thoughts/` using OKF frontmatter, which are later compacted into rules.
6.  **ChatDB (Symbolic Graph API)**: Exposing a parameterized, safe graph traversal tool with visited-set cycle tracking to prevent infinite loops on circular links.
7.  **Generative Search (Sigmoid-Gated Multiplicative Formula)**: 
    *   *Formula*: $\text{Score} = \text{Similarity} \cdot \left( w_{\text{imp}} \cdot \frac{\text{Importance}}{10.0} + w_{\text{rec}} \cdot e^{-\alpha \Delta t} \right) \cdot \text{TierBoost} \cdot \text{Gate}(\text{Similarity})$.
    *   *Sigmoid Gate*: To prevent step-function boundary instability, the search score is scaled by a smooth sigmoid gate: $\text{Gate}(\text{Similarity}) = \frac{1}{1 + e^{-20(\text{Similarity} - 0.60)}}$.
8.  **Self-RAG (Hybrid Hydration Hook)**: 
    *   To eliminate two-step LLM turn latency, we use **Hybrid Hydration**:
        *   *High-Relevance Nodes ($\ge 0.80$)*: Fully hydrated directly into the hook prompt.
        *   *Mid-Relevance Nodes ($[0.60, 0.80)$)*: Indexed in a lightweight table with their titles, scopes, and 1-sentence summaries.
        *   *Low-Relevance Nodes ($< 0.60$)*: Omitted entirely.
9.  **OKF Vault Watcher & Content-Hash Sync Suppression**:
    *   *Sync Model*: Auto-syncs markdown files to SurrealDB by parsing wikilinks and YAML edges.
    *   *Feedback Loop Resolution*: We abandon time-based leases. The daemon registers the `SHA-256` content-hash of files it writes in an in-memory registry. The Watcher computes the hash of file changes; if it matches a registered hash, the watcher ignores the event and removes it from the registry. This is 100% deterministic and race-free.
    *   *Write-Behind Coalescing Queue*: Decoupled, asynchronous writes are queued and flushed atomically over a sliding $5$-second window.

### Key Assumptions & Boundaries
-   **Client-Server Daemon Routing**: To prevent RocksDB file lock crashes in multi-process subagent environments, `SurrealBackend` operates in **Client Mode** when the daemon port (default 8090) is active, routing all queries via HTTP to the running daemon.
-   **Context Pinning**: Wisdom rules, high-importance nodes ($\ge 8.0$), active handoffs, and active STM are pinned in RAM (immune to LRU eviction).
-   **Decoupled BPE Tokenizer**: Use a dedicated local BPE tokenizer corresponding to the active LLM's vocabulary for $\ge 99\%$ accurate token budget counting.
-   **Transactional STM**: Enforce transaction blocks (`BEGIN TRANSACTION ... COMMIT TRANSACTION;`) in all STM routes to prevent cross-subagent data races.

---

## 2. Requirements

### Desired Outcome
An intelligent, self-healing, and race-free memory engine that maintains perfect structural alignment between a human-readable Obsidian vault and an AI-traversable SurrealDB graph, operating with >70% token economy, minimal turn latency, and zero technical debt.

### In Scope
-   Extending the database schema to support `belief_state`, `thought_node`, and `symbol_archive`.
-   Implementing OKF parsing for Obsidian wikilinks and YAML edges in the watcher.
-   Refactoring the pre-invocation hook to return a gated, weighted Self-RAG hybrid hydration prompt.
-   Exposing symbolic query and hydration tools in the MCP routes.
-   Building the MemoryOS paging manager with virtual skeleton parsing, paging-aware editing tools, automated LRU page eviction, and context pinning.
-   Implementing the POMDP belief state and TiM thought abstraction loops.
-   Implementing a sigmoid-gated multiplicative generative search formula and background reflection.
-   Implementing client-server auto-routing in `SurrealBackend` to prevent RocksDB lock crashes.
-   Integrating a dedicated BPE tokenizer, a SHA-256 content-hash sync suppression registry, and transactional STM.

### Out of Scope
-   Replacing the RocksDB or SurrealDB storage engines.
-   Modifying the core ONNX embedding model or runtime.
-   Modifying unrelated CLI commands.

### Falsifiable Acceptance Criteria
1.  **Self-RAG Hook**: Pre-invocation hook returned payload must contain the full text of nodes with similarity $\ge 0.80$, and only index rows (with summaries) for nodes in $[0.60, 0.80)$, without automatic symbol restoration.
2.  **Active Hydration**: Invoking `query_memory(action="hydrate", node_ids=[...])` must return the complete text of the targeted nodes.
3.  **OKF Watcher Sync**: Creating a markdown file with Obsidian links must differentially sync SurrealDB relations, utilizing SHA-256 hash suppression to prevent sync loops, and preserving existing cognitive metadata.
4.  **Generative Retrieval**: Querying the database must return a blended score using the sigmoid-gated multiplicative formula, ensuring smooth transitions to $0.0$ below $0.60$.
5.  **MemoryOS Paging (Virtual & Paging-Aware)**: Large file reads must return virtual skeletons, leaving disk files 100% untouched. Calling `replace_file_content` with a target containing placeholders must successfully fetch the bodies, reconstruct the unpaged block in memory, and edit the physical file on disk.
6.  **Context Pinning**: Pinned pages (Wisdom, Importance $\ge 8.0$) must remain in the active context when an LRU eviction is triggered.
7.  **ChatDB Symbolic Query**: Invoking `query_memory(action="query_symbolic", ...)` must execute a structured graph traversal with visited-set cycle tracking and return matching record IDs.
8.  **POMDP & TiM**: The belief state must update after every tool execution, and thought nodes must be written to `wiki/thoughts/` upon task completion.
9.  **Client-Server Routing**: Initializing a subagent when the daemon is running must route queries via HTTP without crashing on RocksDB lock acquisition.

---

## 3. Design

### 3.1 Data Schemas & Rust Structs

#### SurrealDB Schema Additions (`src/db/schema.rs`)
```sql
-- Metadata extensions
DEFINE FIELD IF NOT EXISTS importance ON episode TYPE option<float> DEFAULT 5.0;
DEFINE FIELD IF NOT EXISTS importance ON wiki_node TYPE option<float> DEFAULT 5.0;
DEFINE FIELD IF NOT EXISTS importance ON wisdom TYPE option<float> DEFAULT 5.0;
DEFINE FIELD IF NOT EXISTS last_retrieved_at ON wiki_node TYPE option<string>;
DEFINE FIELD IF NOT EXISTS last_retrieved_at ON wisdom TYPE option<string>;

-- POMDP Belief State
DEFINE TABLE IF NOT EXISTS belief_state SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS session_id ON belief_state TYPE string;
DEFINE FIELD IF NOT EXISTS tasks_todo ON belief_state TYPE array<string>;
DEFINE FIELD IF NOT EXISTS hypotheses_tested ON belief_state TYPE array<string>;
DEFINE FIELD IF NOT EXISTS confidence_score ON belief_state TYPE float DEFAULT 0.5;
DEFINE FIELD IF NOT EXISTS uncertainty_areas ON belief_state TYPE array<string>;
DEFINE FIELD IF NOT EXISTS updated_at ON belief_state TYPE datetime DEFAULT time::now();
DEFINE INDEX IF NOT EXISTS bs_session ON belief_state FIELDS session_id UNIQUE;

-- Thought Nodes (TiM)
DEFINE TABLE IF NOT EXISTS thought_node SCHEMAFULL;
DEFINE FIELD IF NOT EXISTS title ON thought_node TYPE string;
DEFINE FIELD IF NOT EXISTS content ON thought_node TYPE string;
DEFINE FIELD IF NOT EXISTS scope ON thought_node TYPE string DEFAULT 'general';
DEFINE FIELD IF NOT EXISTS vault_path ON thought_node TYPE string DEFAULT '';
DEFINE FIELD IF NOT EXISTS embedding ON thought_node TYPE option<array<float>>;
DEFINE FIELD IF NOT EXISTS created_at ON thought_node TYPE datetime DEFAULT time::now();

-- Schemafull Relations (Prevents Graph Pollution)
DEFINE TABLE relates_to SCHEMAFULL TYPE RELATION 
    IN (episode, wiki_node, wisdom, entity, thought_node) 
    OUT (episode, wiki_node, wisdom, entity, thought_node);
DEFINE FIELD strength ON relates_to TYPE option<float> DEFAULT 1.0;
DEFINE FIELD relation ON relates_to TYPE string DEFAULT 'related';
DEFINE FIELD created_at ON relates_to TYPE datetime DEFAULT time::now();
```

---

## 4. Precise Codebase Reuse & Refactoring Roadmap

To prevent the introduction of redundant modules or duplicate logic, the development team must build directly on the following existing foundations:

### 4.1 Virtual Paging & Paging-Aware Editing
*   **Target File**: `mythrax-core/src/cognitive/paging.rs`
    *   *Existing:* Has `extract_symbols`, `page_code_block`, and `intercept_and_restore_symbols`. It saves symbols to the `symbol_archive` table and replaces bodies with placeholders on disk.
    *   *Refactoring Roadmap:*
        1.  Refactor the file-reading MCP route `view_file` (in `mcp_routes.rs`) to read physical files, pass them through `page_code_block`, and return virtual skeletons *in memory* (leaving disk clean).
        2.  Refactor the editing MCP routes (`replace_file_content` and `multi_replace_file_content`) to scan `TargetContent` for placeholders, query `symbol_archive` to reconstruct the unpaged target block, and apply the clean patch to the physical file.
        3.  Implement the `PagingManager` with LRU eviction and context pinning.

### 4.2 Sigmoid-Gated Search, Cycle-Proof Traversals & Auto-Routing
*   **Target File**: `mythrax-core/src/db/backend.rs`
    *   *Existing:* Implements exponential decay on episode utility and calculates `blended_score`. Has `relate_nodes` for creating database relations.
    *   *Refactoring Roadmap:*
        1.  Replace the old blended score with the new sigmoid-gated multiplicative scoring formula. Maintain the background tokio spawn pattern to update `last_retrieved_at` and the decayed `utility` in SurrealDB, ensuring these metadata updates are never written back to disk.
        2.  Implement the cycle-proof graph query (`query_symbolic`) using BFS/DFS with a scoped `visited: HashSet<String>` of record IDs to prevent infinite loops on circular Obsidian links.
        3.  Refactor `SurrealBackend::new` to check if port 8090 is active, and if so, route all operations via `reqwest` HTTP requests to the running daemon instead of attempting to open direct RocksDB file locks, preventing multi-process crashes.

### 4.3 Memory Consolidation & Thought Compaction
*   **Target Files**: `mythrax-core/src/cognitive/compactor.rs` & `synthesis.rs`
    *   *Existing:* `compactor.rs` has `compact_scope` (DBSCAN clustering and LLM compaction). `synthesis.rs` has `DreamCoordinator::run_dream` (offline dreaming to synthesize episodic memories into wisdom rules).
    *   *Refactoring Roadmap:*
        1.  Extend `compact_scope` and `run_dream` to inspect the decayed `utility` of episodes and wiki nodes, automatically archiving nodes with `utility < 2.0` (deleting from SurrealDB search and moving to an `archive/` folder).
        2.  Add a step to load, cluster, and synthesize raw thoughts in `wiki/thoughts/` into permanent rules, deleting the raw thought files.

### 4.4 Hash-Based Sync Suppression & Coalescing Queue
*   **Target Files**: `mythrax-core/src/vault/watcher.rs` & `markdown.rs`
    *   *Existing:* `watcher.rs` has notify-based file watching and `WatchIgnoreList` with a 2-second lease. `markdown.rs` has `parse_frontmatter` and `extract_plain_text`.
    *   *Refactoring Roadmap:*
        1.  Replace the time-based lease in `WatchIgnoreList` with a cryptographic **SHA-256 Content-Hash Suppressor** registry to prevent self-triggering loops.
        2.  Implement the **Write-Behind Coalescing Queue** with a sliding 5-second window.
        3.  Refactor `markdown.rs` to extract Obsidian wikilinks and YAML edges, and refactor `sync_file_to_db` in `watcher.rs` to parse them and create `relates_to` edges in SurrealDB. To prevent split-brain overrides, ensure `sync_file_to_db` uses `UPDATE ... MERGE` when syncing, preserving existing cognitive metadata fields (utility, last_retrieved_at) in SurrealDB.

### 4.5 Hybrid Hydration Hook, POMDP & Transactional STM
*   **Target File**: `mythrax-core/src/mcp_routes.rs`
    *   *Existing:* Exposes the `pre_invocation_hook` which syncs state, checks for pending subagent handoffs, and retrieves stashed STM variables.
    *   *Refactoring Roadmap:*
        1.  Refactor the hook to implement the three-tier **Self-RAG Hybrid Hydration** strategy.
        2.  Retrieve the `BeliefState` from SurrealDB and inject it at the top of the hook prompt.
        3.  Add a post-execution hook to update the belief state after tool executions.
        4.  Refactor STM routes (`put`, `handoff`) to use SurrealDB transactional blocks (`BEGIN TRANSACTION ... COMMIT TRANSACTION;`) to ensure safe parallel subagent execution.

---

## 5. Test Plan & Implementation Tasks

### Phased Refactoring Checklist

#### T1.1: Schema & Struct Updates
-   **Actions**:
    -   Modify `mythrax-core/src/db/schema.rs` with new schemafull tables, indices, and constraints.
    -   Add `BeliefState` and `ThoughtNode` structs to `contracts.rs`.
-   **Validation**: Verify daemon compiles and boots without errors.

#### T1.2: Client-Server Auto-Routing
-   **Actions**:
    -   Implement the client-server auto-routing check in `SurrealBackend::new`. If the daemon port is active, route all database queries via HTTP `reqwest` client to the daemon instead of opening a direct RocksDB connection.
-   **Validation**: Spawn a subagent while the main daemon is running and verify it successfully routes queries without crashing on RocksDB locks.

#### T2.1: Sigmoid-Gated Search & BPE Tokenizer
-   **Actions**:
    -   Implement the sigmoid-gated multiplicative search formula in `src/db/backend.rs`.
    -   Integrate a proper local BPE tokenizer (using the `tokenizers` crate in dependencies) in the token budget counter to replace the naive fallback, loading the tokenizer corresponding to the active LLM.
-   **Validation**: Run `test_sigmoid_gated_search` and assert 100% token count accuracy.

#### T3.1: OKF Watcher, SHA-256 Suppression & Coalescing Queue
-   **Actions**:
    -   Refactor `watcher.rs` and `markdown.rs` to parse Obsidian body links and YAML edges.
    -   Implement the **Write-Behind Coalescing Queue** and the **SHA-256 hash ignore set** in `watcher.rs`.
    -   Refactor `sync_file_to_db` in `watcher.rs` to use `UPDATE ... MERGE` when syncing, preserving existing cognitive metadata.
-   **Validation**: Run `test_okf_watcher_sync` and verify graph sync operates without feedback loops.

#### T3.2: Paging-Aware MCP & Virtual Paging
-   **Actions**:
    -   Implement **virtual in-memory paging** in the file-reading routes (`view_file`), returning skeletal structures to the client while keeping files on disk unmodified.
    -   Refactor `replace_file_content` and `multi_replace_file_content` in the MCP layer to scan for page placeholders, surgically hydrate them from `symbol_archive` in memory, and apply the clean patch to the physical file.
    -   Expose the `swap_in` and `swap_out` active paging tools in `mcp_routes.rs`.
    -   Implement the MemoryOS paging manager with LRU eviction and context pinning.
-   **Validation**: Run `test_virtual_paging_editing_flow` and verify that the agent can successfully edit virtually paged files without disk corruption.

#### T4.1: Cycle-Proof ChatDB Graph API & HTR
-   **Actions**:
    -   Implement `action="query_symbolic"` in `query_memory` with visited-set cycle tracking.
    -   Implement POMDP belief state updates and TiM thought abstraction.
    -   Implement shared STM real-time propagation for parallel HTR runs using transaction blocks.
-   **Validation**: Verify multi-depth graph queries traverse cyclic links safely.
