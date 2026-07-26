# Specification: Arbor Framework Alignment & Single-Pass Chunked Ingestion

## 1. Overview
Align Mythrax core memory architecture with the Arbor framework research paper (`arXiv:2606.11926v1.pdf`) and resolve performance bottlenecks during high-throughput transcript ingestion. This spec incorporates findings from three adversarial CTO reviews.

---

## 2. Functional Requirements

### A. Arbor 4-Field Memory Node Schema ($n = \langle h_n, r_n, \iota_n, \mu_n \rangle$)
- **Hypothesis ($h_n$)**: Target research direction or user intent claim.
- **Factual Result & Evidence ($r_n$)**: **Uncompressed raw observations** (compiler error tracebacks, diffs, test outputs) preserved verbatim without LLM truncation.
- **Distilled Insight ($\iota_n$)**: Causal lesson answering *what worked* (positive assumptions) and *what failed* (negative constraints).
- **Metadata & References ($\mu_n$)**: Exact file paths, symbol locations, git branch references, and metrics.
- Enforce explicit 4-field tuple accessors across `HypothesisNode`, `WikiNode`, and `Episode` structs.

### B. High-Speed Single-Pass Chunked Ingestion
- Scan all transcript directories in a single pass without HTTP pagination pauses.
- Parse internal JSONL timestamps (`created_at` / `timestamp`) across all turns for deterministic chronological sorting (oldest to newest).
- Execute SurrealDB insertions in **50-episode transactional chunks** (`episodes.chunks(50)`).
- **Chunk failure strategy**: Skip failed chunks, log errors, continue (eventual consistency via re-ingestion). The pipeline is idempotent — `existing_titles` dedup ensures subsequent runs cleanly pick up stragglers.
- Batch inverted index IDF updates into set-based SQL queries (`UPDATE idf_index ...`) eliminating sequential 1,000-query loops.
- Suppress filesystem watcher (`watcher.rs`) sync when `IS_INGESTING == true` to prevent RocksDB write lock collisions.
- Wire `skip_llm` parameter (currently dead-assigned at `ingestion.rs:613`) to bypass LLM extraction steps.

### C. Explicit Coordinator Tool Boundaries (Arbor Table 6)
The Arbor coordinator must mutate the hypothesis tree **exclusively** via typed MCP tools. Direct JSON/DB manipulation is forbidden.
- **`TreeAddNode`**: Create child hypothesis nodes under a specified parent.
- **`TreeUpdateNode`**: Update node fields (status, score, insight, result).
- **`TreePrune`**: Mark a node as pruned, persist negative constraints as `WisdomRule`.
- **`TreeView(format=...)`**: Query tree state in multiple formats:
  - `compact` — one-line-per-node summary
  - `full` — complete node details with all 4 fields
  - `node` — single node by ID
  - `pending` — pending/actionable nodes only
  - `constraints` — aggregated negative lessons from failed/pruned nodes
- **`GitMergeBranch`**: Explicitly callable held-out validation and merge gate (not automatic).

### D. Explicit FSM Lifecycle (`IDEATE → EXECUTE → EVALUATE → PRUNE/MERGE`)
- Wrap the coordinator in a strict Finite State Machine enforcing the Arbor lifecycle.
- **Depth limit**: Configurable `max_depth` parameter (default: 2).
- **Budget tracking**: Token, wall-clock, and iteration budget caps with graceful shutdown on exhaustion.

### E. Parallel Executor Dispatch (`RunSubagentParallel`)
- Dispatch 2–4 executors concurrently on independent tree nodes in isolated worktrees.
- Each executor must acquire the `METAL_GPU_SEMAPHORE` before running GPU-bound operations.
- `dispatch_batch` must respect the semaphore — `buffer_unordered(2)` is unsafe for GPU workloads without semaphore coordination.

### F. Upward Insight Propagation (`TreePropagate`)
- Encapsulate the propagation strategy under a unified `TreePropagate` trait.
- Traverse parent-child graph edges (`parent_id` / `relates_to`) during scope compaction.
- Abstract child leaf insights into parent scope nodes: $\iota_{\text{parent}} \leftarrow \text{Abstract}(\{\iota_{\text{children}}\})$.
- Export accumulated `Negative Constraints` into root scope compaction notes.

### G. Negative Constraint Ideation Gating
- `TreeView(format="constraints")` aggregates negative lessons from failed/pruned nodes into explicit constraint blocks that condition Cloud Brain dreaming and ideation.
- **Migration**: Rewire `handle_pre_invocation_hook` (L1790) from `collect_policy_context` to `TreeView(format="constraints")` before removing the legacy function.

### H. Background Related-Work Annotation (`SearchIdeaContext`)
- Background worker dispatched by the coordinator to annotate tree nodes with related work asynchronously, without blocking the `IDEATE` step.

### I. Convergence Detection & Parent Exhaustion
- **Sliding window**: Compute $\Delta \text{score} / \Delta \text{visits}$ over the last 5 node evaluations.
- **Escalating signals**: `warn` → `paradigm_shift` → `stop`.
- **Parent exhaustion**: Flagged when all child executions fail to improve the baseline. Triggers fresh depth-1 exploration branches.

### J. Held-Out Validation Gate (`Etest` / `GitMergeBranch`)
- Refactor `HeldOutEvaluator` into formal `Etest` struct.
- `GitMergeBranch` creates a detached worktree, runs `eval_cmd_test`, merges **only** if $S_{test} > S_{test}(M_{best})$.
- Exposed as a callable MCP tool — the coordinator explicitly chooses when to attempt a merge.

### K. Distillation Prompt Upgrade (4-Field Arbor Contract)
- Replace the generic `"You are a code summarizer"` system prompt in `run_summarization_task` (distillation.rs L159) with the explicit 4-field structural contract:
  ```
  ### 🎯 Hypothesis & Intent (hn)
  ### 📊 Factual Result & Raw Evidence (rn)
  ### 🧠 Distilled Insight & Causal Lessons (ιn)
  ### 🔑 Artifact References & Key Symbols (µn)
  ```
- Extend `enforce_symbol_integrity` to protect both `rn` and `µn` blocks verbatim.

---

## 3. Dead Code Removal Schedule

| Code | File | Lines | Reason | Prerequisite |
|------|------|-------|--------|-------------|
| `backpropagate_insights` | `arbor.rs` | 458–514 | Superseded by `TreePropagate` trait | `TreePropagate` implemented |
| `decide_admission` | `arbor.rs` | 516–585 | Superseded by `GitMergeBranch` MCP tool | `GitMergeBranch` tool implemented |
| `collect_policy_context` | `manage_handlers.rs` | 2196–2300 | Replaced by `TreeView(format="constraints")` | `handle_pre_invocation_hook` L1790 rewired |
| `let _ = skip_llm;` | `ingestion.rs` | 613 | Dead assignment masking feature flag | Wire `skip_llm` to title generation bypass |

---

## 4. Architectural Risk Mitigations

| Risk | Evidence | Mitigation |
|------|----------|------------|
| RocksDB lock panics during ingestion | `watcher.rs` L670-678 missing `IS_INGESTING` check | Add `IS_INGESTING` guard at top of `sync_file_to_db` |
| GPU OOM via concurrent evaluation | `dispatch_batch` L588 `buffer_unordered(2)` | Acquire `METAL_GPU_SEMAPHORE` in executor, or restrict to `buffer_unordered(1)` |
| Tokio deadlock in `LlmCriticEvaluator` | `arbor.rs` L114 nested runtime in `std::thread::spawn` | Refactor to native async with `tokio::task::spawn_blocking` |
| Chunk transaction failure | `crud_operations.rs` chunked commits | Skip failed chunks, log, continue (idempotent re-ingestion) |

---

## 5. Non-Functional Requirements
- **Performance**: Ingest 1,000+ conversation turns into SurrealDB and Markdown files in under 5 seconds.
- **Integrity**: Zero loss of raw compiler error tracebacks during summarization.
- **Safety**: Zero RocksDB lock panics or SurrealDB transaction timeouts.
- **Resumability**: Ingestion is idempotent — re-runs skip already-processed episodes.
