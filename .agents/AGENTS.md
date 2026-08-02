# Mythrax Workspace Rules

## Parallel Test Execution
- **Mandate**: Always run test suites in parallel using `cargo nextest run` or the `cargo t` alias.
- **Why**: The default `cargo test` runs test suites sequentially which triggers database lock contentions and significantly slows down the E2E verification loop.
- **Isolation Mandate**: When running parallel subagents or concurrent test suites, each subagent MUST execute in a separate git worktree or specify a unique `CARGO_TARGET_DIR` (e.g., `CARGO_TARGET_DIR=target/track_a`) and isolated temp DB directory (e.g., `/tmp/track_a`) to prevent cargo target lock contention and database lock conflicts.
- **Fast Mocking & Fast Domain Iteration**: Always specify `MYTHRAX_TEST_MOCK=1` when running tests. During iterative coding/debugging loops, agents MUST NOT run the full 310-test suite. Agents MUST run ONLY the specific targeted domain harness (`cargo nextest run -p mythrax-core domain_<subsystem>`) or a single test filter (`-E 'test(test_name)'`). Run the full test suite ONLY once at the very end of task execution.
  - `MYTHRAX_TEST_MOCK=1 cargo nextest run -p mythrax-core domain_cognitive` (Cognitive & Compactor changes)
  - `MYTHRAX_TEST_MOCK=1 cargo nextest run -p mythrax-core domain_search_retrieval` (Search, BM25, & Scored Retrieval)
  - `MYTHRAX_TEST_MOCK=1 cargo nextest run -p mythrax-core domain_vault_storage` (Vault, CRUD, & Ingestion)
  - `MYTHRAX_TEST_MOCK=1 cargo nextest run -p mythrax-core domain_hooks_models` (Hooks, Broker, & Routing)

## Core System Goals & Objectives

To fulfill its role as a persistent, autonomous sidecar intelligence companion, Mythrax commits to four fundamental objectives:

1. **Short-Term Context Recall & Compaction Recovery:** Provide immediate short-term retrieval for active agents operating with large context windows. Memory compaction must preserve the granular sequence of raw turns (user inputs, assistant thoughts, tool outputs) so agents can review their immediate past steps and avoid forgetting loops.
2. **Project-Level Memory (Insights):** Build high-cohesion, project-specific knowledge representations (`wiki_node` / clusters) so that multiple agents or sequential sessions on the same codebase share operational constraints and context.
3. **Cross-Project Global Memory (Wisdom):** Maintain a durable, global partition (`wisdom`) for general guidelines, coding practices, user preferences, and architectural rules that apply universally across workspaces (e.g. general design principles).
4. **Forged Knowledge & Skill Integration:** Enable raw reference assets (like PDFs, specs, and papers) and composed agent strategies (e.g. chaining `spec-builder`, `loop-builder`, and reviewers) to be dynamically injected via RAG into active context windows on-demand.

## Mythrax 6-Signal Unified Retrieval (v2.5.2)
- **6 Retrieval Signals**: Combine Vector Similarity, BM25 (FTS) Relevance, Concept Spreading Activation, STM Working Memory Injection, Temporal Neighbor Expansion, and Gaussian Temporal Proximity.
- **Concept Spreading Activation**: Attenuates scores as it traverses `relates_to` edges cross-scope (`anchor_sim * edge_confidence * 0.5`).
- **STM Working Memory Injection**: Query active STM KV pairs, compare query embedding with values, and inject matching entries as high-priority candidates (`tier: "working"`).
- **Temporal Neighbors**: Expand candidates by traversing `followed_by` temporal relationship edges.
- **Gaussian Temporal Proximity decay**: Replace hard time-demotions with \(\exp(-\Delta t^2 / 2\sigma^2)\) scoring, default \(\sigma = 168h\).
- **Active VRAM Model Broker**: Dynamic coordination unloads embedding models before loading reranking/inference models to prevent OOM.
- **Cross-Scope Graduation**: Promotes project-scoped insights and procedural episodes (365-day half-life, 500-node LRU cap) to generalized global wisdom rules upon convergence across multiple projects (cosine \(\ge 0.85\)).

## Thinking and Writing Concision (docs:write-concisely)
- **Mandate**: All agents (including subagents) MUST apply the `/docs:write-concisely` Strunk & White principles to all outputs, including **inner thoughts (thinking blocks)**, planning documents, and formal vault markdown files.
- **Rules**:
  - **Omit needless words**: Be direct, clean, and concise. Eliminate throat-clearing, introductory fillers, and repetitive summaries.
  - **Use active voice**: Make the subject perform the action to keep descriptions vigorous.
  - **Use positive form**: Make definite assertions instead of evasive/negative qualifiers.
  - **Use definite, specific, concrete language**: Avoid vague generalizations.
  - **Keep paragraphs focused**: Stick to one topic per paragraph.

## Rust Coding Standards & Architecture Directives
- **Strict Anti-Lazy Implementation Mandate**: Coding subagents are strictly forbidden from taking lazy design shortcuts, writing temporary sync/async bridge wrappers, inserting `// TODO` or placeholder stubs, omitting error handling on edge cases, or applying band-aid patches to pass individual tests. All implementations must be fully written out, architecturally sound, type-safe, and natively integrated across all downstream callsites.
- **Direct Native Async Refactoring (Anti-Bridge Rule)**: Coding subagents MUST NOT create parallel `_async` methods alongside existing sync methods, nor use `futures::executor::block_on` or `tokio::task::block_in_place` fallbacks inside default trait methods. When converting a trait or subsystem to async, subagents MUST update the trait definition directly with `async fn` and refactor all downstream callsites natively.
- **Top-Level Scoping & Safe RAII Guards**: Operational status guards (e.g. `IS_INGESTING`) and cleanup routines MUST be scoped at the outermost public entry point of a function, covering all match arms, harness types, and execution branches. All temporary database state (e.g. `pipeline_cluster`) or filesystem resources MUST use safe RAII scope guards (implementing `Drop` with `Arc<dyn Trait>` handles) so cleanup is guaranteed on early `?` error returns, panics, and scope drops. Unsafe raw pointer transmutes (`*const dyn Trait`) are strictly forbidden.
- **Strict Lock Ordering & Contention Prevention**: Subagents MUST NOT hold a primary lock (e.g., `EMBEDDING_CACHE` or `term_counts_cache`) while acquiring a secondary lock (e.g., `SQLITE_CACHE_CONN` or inner scope locks). Always extract required data into local variables, drop the primary lock completely, and then acquire secondary locks or execute I/O operations.
- **Algorithmic Complexity & Bulk Operations (No $O(N)$ Hot-Path Scans)**: Subagents MUST NOT perform $O(N)$ linear iteration scans (e.g. `.min_by_key()`) inside hot-path loops or per-element insertions. Use constant-time $O(1)$ data structures (e.g. `lru::LruCache`) or perform bulk pruning (evicting the bottom 10% of items in a single pass when capacity is reached).
- **Complete Resource Lifecycle & Write-on-Evict Safety**: Any component that loads GPU VRAM weights or allocates heavy in-memory buffers MUST implement a public `evict()` method and register it with the background idle eviction loop (`daemon.rs`). Any cache eviction mechanism (such as `LruCache::push` or `resize`) MUST inspect evicted items and immediately persist dirty entries to disk before dropping them from memory (Write-on-Evict).
- **Anti-AI Slop & Quality Gate Directives**:
  - **Strict Assertion Mandate**: All test assertions MUST evaluate explicit struct field values, exact error types, and verified database mutations. Generic `assert!(res.is_ok())` and `assert!(res.is_some())` checks are categorized as AI Slop and will fail review.
  - **Panic & Stub Elimination**: Production paths must contain zero `.unwrap()`, `.expect()`, `todo!()`, `unimplemented!()`, or `// TODO` stubs.
  - **Lock Contention Guarantee**: Mutex and RwLock guards must be dropped before any `.await` point or secondary lock acquisition.
  - **RAII Resource Boundaries**: All state flag mutations and resource handles must use `Drop`-implementing scope guards to guarantee clean state recovery on panics or early `?` error returns.
  - **Write-on-Evict Cache Durability**: In-memory LRU evictions must synchronously flush dirty records to disk before dropping items from RAM.
- **Incremental Per-Phase Git Commit & Push Mandate**: Agents MUST execute a git commit and `git push origin <branch_name>` immediately upon completing each phase of a track (after verifying unit tests and build status) before proceeding to subsequent phases or triggering code reviews. This prevents multi-commit push backlogs and keeps remotes continuously up to date.
- **Mandatory Inter-Phase Formal Review & Unconditional CTO Approval Gate**: Agents MUST execute a formal review (`/conductor-review`) between development phases, spin up the Adversarial CTO Reviewer subagent, and iteratively remediate all findings (including minor items) in a loop until the CTO Reviewer issues an explicit, unconditional `APPROVED` verdict. Once approved, the agent MUST immediately execute a git commit and push to the feature branch (`mythrax-3.1-release`) before starting the next phase.
- **CTO Reviewer Anti-Work-Avoidance & Decomposed Task Structuring Mandate**: When auditing specs, plans, or implementations, the CTO Reviewer is strictly prohibited from recommending scope reduction, feature removal, or deferring work because a task is perceived as "too complex," "massive," or "tedious." Instead, the CTO Reviewer MUST handle complexity by instructing the agent to decompose large phases and complex features into smaller, bite-sized phases and scoped sub-tasks (< 32k tokens per task), guaranteeing 100% feature completeness without scope erosion.


