# Specification: Advanced Agentic Memory Systems Integration (v1.2)

This document defines the formal engineering specification for integrating next-generation agentic memory paradigms into the Mythrax platform. It establishes the technical requirements, structural designs, testing methodologies, and implementation tasks required to build a highly performant, self-regulating, and cognitively consistent memory substrate.

---

## 1. Clarify & Objectives

### Restated Request
Evolve the Mythrax memory architecture by integrating advanced cognitive memory paradigms, maintaining the existing client-server daemon topology, ONNX embedding centralization, and SurrealDB graph engine. Consolidate the MCP tools and CLI command namespaces to eliminate interface bloat, restore the Forge AI-driven extraction pipeline, and dynamically inject capabilities wisdom into the agent context.

### Core Paradigms & Architecture (v1.2)

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
    *   *Feedback Loop Resolution*: We abandon time-based leases. The daemon registers the `SHA-256` content-hash of files it writes in an in-memory registry. The Watcher computes the hash of file changes; if it matches a registered hash, the watcher ignores the event and removes it from the registry.
10. **Forge AI Ingestion Pipeline (Restored & Optimized)**:
    *   *Paragraph Semantic Chunker*: Splits documents into 1,000–2,000 token semantic segments using paragraph boundaries, with line-based and token-based fallbacks for large segments.
    *   *Parallel Batch Embedding*: Parent document and all chunks are embedded in a single parallel pass (`embed_batch`) before database insertion, eliminating ONNX runtime lock contention.
    *   *AI-Driven Extraction*: For each chunk, the LLM is invoked to extract system-level **Wisdom Rules** and **Wiki Concepts**.
    *   *Hierarchical Graph Linking*: Establishes `relates_to` edges linking Chunks ➔ Parent Document (`relation: "parent"`), sequential adjacent chunks (`relation: "next"`, `relation: "prev"`), and extracted rules/concepts ➔ originating chunk (`relation: "extracted_from"`).
11. **Pre-Invocation Capabilities Injection**:
    *   The `pre_invocation_hook` retrieves all permanent/system-level wisdom rules (`SELECT * FROM wisdom WHERE tier = 'permanent';`) and injects them under a dedicated section (`### 🛠️ Mythrax Capabilities & Tool Wisdom`) at the top of the context, ensuring the agent always understands RocksDB locks, subagent handoffs, and virtual paging.

---

## 2. Requirements & Acceptance Criteria

### In Scope
-   Extending the database schema to support `belief_state`, `thought_node`, `chat_history`, and `symbol_archive`.
-   Implementing the paragraph-boundary semantic token chunker in `forge.rs`.
-   Pre-computing embeddings for all ingestion nodes using parallel `embed_batch`.
-   Restoring Wisdom Rule and Wiki Concept extraction on a per-chunk basis in `forge.rs`.
-   Consolidating the MCP tools to a **7-tool architecture**:
    1.  `manage_memory` (search, rules, nodes, root, query_symbolic, save, feedback, thought)
    2.  `manage_file` (view, replace, multi_replace)
    3.  `manage_htr` (init, ideate, execute, backprop, merge, run)
    4.  `manage_stm` (put, get, clear, handoff)
    5.  `manage_vault` (verify, organize, reprocess, summarize, audit, ingest_bulk, ingest_forge)
    6.  `manage_config` (get, set)
    7.  `pre_invocation_hook`
-   Consolidating the `mythrax` CLI subcommands to align 1-to-1 with the MCP tool namespaces, nesting `ingest` and `audit` under `mythrax vault`.
-   Refactoring the pre-invocation hook to dynamically retrieve and inject permanent capabilities wisdom.

### Falsifiable Acceptance Criteria
1.  **Forge AI Extraction**: Ingesting a document must successfully generate a parent node, semantic chunk nodes, extracted Wisdom Rules, and extracted Wiki Concepts in SurrealDB and on disk, with correct `relates_to` edges (`parent`, `next`, `prev`, `extracted_from`).
2.  **Batch Embedding Ingestion**: Ingesting $N$ chunks must execute a single parallel `embed_batch` ONNX inference call instead of $N$ sequential `embed` calls.
3.  **MCP Memory Consolidation**: Calling the consolidated `manage_memory` tool with actions `search`, `save`, `feedback`, or `thought` must route correctly to their respective read/write handlers.
4.  **MCP Ingestion Consolidation**: Calling `manage_vault` with actions `ingest_bulk` or `ingest_forge` must trigger the respective ingestion pipelines.
5.  **CLI Consistency**: Running `mythrax vault ingest-forge <file>` and `mythrax vault audit` must execute the forge and compliance audit tasks cleanly.
6.  **Capabilities Wisdom Injection**: The pre-invocation hook returned context must contain the formatted text of all permanent wisdom rules (e.g., RocksDB lock, safe deletions, and v1.2 capabilities).
7.  **Compiling Test Suite**: Running `cargo test` in the `mythrax-core` crate must compile and pass all 68+ tests without regression.

---

## 3. System Architecture & Component Mapping

### 3.1 7-Tool MCP Schema Specification

#### 1. `manage_memory`
*   **Purpose**: Unifies all semantic memory search, traversal, and logging.
*   **Actions**:
    *   `search`: Semantic vector search on episodes and wiki nodes.
    *   `rules`: Query active system wisdom rules.
    *   `nodes`: Hydrate a list of memory nodes by record IDs.
    *   `root`: Retrieve the active Obsidian vault root directory.
    *   `query_symbolic`: Perform cycle-proof graph traversals.
    *   `save`: Save a new episodic memory.
    *   `feedback`: Record reinforcement feedback for a rule.
    *   `thought`: Log a raw thought node.

#### 2. `manage_file`
*   **Purpose**: File I/O, virtual in-memory paging, and paging-aware editing.
*   **Actions**:
    *   `view`: Read a file, returning a virtual skeleton with symbol placeholders.
    *   `replace`: Contiguous paging-aware block replacement.
    *   `multi_replace`: Multi-block non-contiguous paging-aware patching.

#### 4. `manage_htr`
*   **Purpose**: Arbor Hypothesis-Tree Refinement workflows.

#### 5. `manage_stm`
*   **Purpose**: Transactional short-term memory and subagent handoffs.

#### 6. `manage_vault`
*   **Purpose**: Vault administrative tasks, maintenance, and document ingestion.
*   **Actions**:
    *   `verify`: DB-to-filesystem self-healing.
    *   `organize`: Renaming and duplicate resolution.
    *   `reprocess`: Reprocessing missing embeddings.
    *   `summarize`: Periodic dreaming and rule synthesis.
    *   `audit`: Safety compliance audits.
    *   `ingest_bulk`: Bulk ingestion of transcript logs.
    *   `ingest_forge`: Forge document ingestion (semantic splitting + AI extraction + batch embedding).

---

### 3.2 Aligned CLI Subcommand Architecture

```
mythrax
├── init [--harness <harness>] [--source <source>] [--non-interactive]
├── daemon [start | run | stop]
├── mcp
├── memory [query | record | feedback | root]
├── htr [init | ideate | execute | backprop | merge | run]
├── stm [put | get | clear | handoff]
├── vault [organize | verify | reprocess | summarize | ingest-bulk | ingest-forge | audit]
└── config [get | set]
```

*Note: The top-level commands `mythrax ingest` and `mythrax audit` are removed. They are refactored as sub-actions under `mythrax vault` (`ingest-bulk`, `ingest-forge`, `audit`), mirroring the `manage_vault` MCP tool.*

---

## 4. Verification & Test Plan

### Test Design (TDD Roadmap)

1.  **`test_forge.rs` (Restored)**:
    *   Assert that ingesting a document extracts Wisdom Rules and Concepts.
    *   Assert that the files are written to the store under `wisdom/forge/` and `wiki/forge/`.
2.  **`test_semantic_document_splitting_relations.rs` (Updated)**:
    *   Assert that the parent and chunk nodes are linked via `relates_to` (`parent`, `next`, `prev`).
    *   Assert that the extracted rules/concepts are linked to their originating chunk node (`relation: "extracted_from"`).
    *   Assert that all nodes are embedded in a single parallel `embed_batch` call.
3.  **`test_stm.rs` (Pre-invocation Hook Capabilities)**:
    *   Insert a mock permanent wisdom rule.
    *   Execute the pre-invocation hook.
    *   Assert that the returned hook context contains the section `### 🛠️ Mythrax Capabilities & Tool Wisdom` and the formatted text of the mock permanent rule.
4.  **`test_cli_e2e.rs`**:
    *   Assert that the consolidated CLI commands (`mythrax memory query`, `mythrax vault ingest-forge`, `mythrax vault audit`) run successfully.
