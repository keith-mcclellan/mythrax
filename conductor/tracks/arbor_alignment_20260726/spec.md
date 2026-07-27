# Specification: Arbor Framework Alignment & Single-Pass Chunked Ingestion

## 1. Overview
Align Mythrax core memory architecture with the Arbor framework research paper (`arXiv:2606.11926v1.pdf`), fix fatal memory integration bugs that prevent agents from learning, and resolve performance bottlenecks during high-throughput transcript ingestion. This spec incorporates findings from 3 adversarial CTO reviews, 1 forensic root-cause investigation, 1 vault/graph UX audit, and 1 deep architectural investigation.

---

## 2. Critical Memory Integration Fixes (EMERGENCY — Before All Other Work)

### A. Guardrail Trigger Bug (`manage_handlers.rs` L1496-1498)
- **Current**: `turn_content.to_lowercase().contains(&rule.target_pattern.to_lowercase())` — exact substring match. Agents never produce the pattern before making the mistake, so rules are never injected.
- **Required**: Replace with semantic similarity (cosine ≥ 0.70) between current turn embedding and rule embeddings. Rules must fire BEFORE the mistake, not after.

### B. Auto-Retrieval Fallback Bug (`manage_handlers.rs` L1727)
- **Current**: `let search_query = query.unwrap_or("general context");` — retrieves pure noise.
- **Required**: Extract user's last message or active task description. Only fall back to generic as last resort.

### C. Utilization Scoring Bug (`manage_handlers.rs` L1411-1447)
- **Current**: `.contains()` check on `wiki.name` / `ep.title`. Always fails → EMA decays importance → memory evicted.
- **Required**: If a memory was injected into context, mark as utilized. Injection IS utilization.

### D. Obsidian-Compatible Graph Edge Representation
- All vault markdown files must represent SurrealDB graph edges as Obsidian `[[wikilinks]]`.
- **Vault-relative paths only** — no `/Users/keith/mythrax-vault/...` absolute paths.
- **No empty-path wikilinks** — `[[|title]]` renders as dead link.
- **Typed relationship sections**: `## Source Episodes`, `## Related Insights`, `## Supersedes`, `## Parent`, `## Children`.
- **Backlinks**: episodes get `## Synthesized Into`; wiki nodes get `## Source Episodes`.
- **Frontmatter uses wikilink paths**, not SurrealDB record IDs.
- **Human-readable filenames**: slugified titles, not UUID strings.

### E. Post-Ingestion Compaction & Vault Cleanup
- Auto-trigger scope compaction after `bulk_ingest_vault` completes.
- Physically move archived episodes from `episodes/` → `archive/`.
- Regenerate MOC.md to expose wiki knowledge base by scope.

### F. Corrupted Wisdom Graduation (`synthesis.rs` L3468-3469)
- **Current**: `action_to_avoid` is set to the same value as `target_pattern`. If a positive pattern like "Use connection pooling" is graduated, agents are told: **"Avoid: Use connection pooling"**.
- **Current**: `causal_explanation` is hardcoded to `"Synthesized via cross-scope graduation."` — no actual causal reasoning preserved.
- **Required**: Use an LLM call to properly synthesize `action_to_avoid` and `causal_explanation` from the cluster content.

### G. Distillation Prompt Doesn't Extract Mistakes (`distillation.rs` L289-295)
- **Current**: LLM is asked to extract: Decisions, Constraints, User Preferences, Summary, Takeaways. It is NEVER asked to extract mistakes, failures, errors, or causal lessons.
- **Required**: Add explicit extraction categories: "Mistakes & Failures", "Root Causes", "What Worked vs What Didn't".

### H. Naive Correction Detection (`precompact.rs` L300-308)
- **Current**: Corrections only detected if user literally says "wrong", "forgot", "mistake", "should have", "actually", "not right". If an agent self-corrects or the user provides implicit correction, it's invisible.
- **Required**: Use semantic similarity or LLM classification to detect corrections, not keyword matching.

### I. Token Budget Silent Eviction (`manage_handlers.rs` L1262-1327)
- **Current**: 8000-token budget permanently archives unpinned episodes when exceeded. No notification to the agent.
- **Required**: At minimum, notify the agent which memories were evicted. Consider increasing the budget or using summarization instead of deletion.

### J. No Automatic Post-Invocation Hook
- **Current**: No `handle_post_invocation_hook` exists. Session reflection relies on a 15-turn boundary heuristic or manual `reflect` trigger.
- **Required**: Implement a proper post-invocation lifecycle that runs a reflection sweep after every session, extracting mistakes and causal insights automatically.

### K. STM Handoff Truncation (`manage_handlers.rs` L98)
- **Current**: Agent-to-agent payloads truncated at 1000 characters (≈250 tokens). Appends `<Value too large for STM. Consult contract file directly.>` but never provides the contract file path.
- **Required**: Raise limit to 32,000 characters minimum, or inject the contract file path into subagent context.

### L. RAPTOR Summary Embedding Gap (`compactor.rs` L1536)
- **Current**: RAPTOR summaries saved with `embedding: None`, relying entirely on filesystem watcher to async-embed. If watcher misses the event, summary is permanently invisible to semantic search.
- **Required**: Embed synchronously after saving, or implement a background reconciliation sweep for nodes with `embedding: None`.

### M. Immediate Mitigation (No Code Change Required)
- Set `MYTHRAX_PRE_INVOCATION_TOKEN_BUDGET=128000` to prevent `p1_advisory.clear()` from wiping all memories. The env var already exists in the code (L1810) but defaults to 3000.

### N. Architecture to PRESERVE (Do Not Break)
- `search_pipeline.rs`: Hybrid BM25 + vector + temporal sigmoid decay — mathematically sound.
- `arbor.rs` UCT selection: Textbook MCTS exploration/exploitation formula — correct.
- `synthesis.rs` DBSCAN clustering: Cosine distances + dynamic eps elbow — correct.
- These algorithms are NOT the problem. The bugs are all in the plumbing/UX layer.

### O. Test Suite Blind Spots
- Backend functions (`backend.search()`) are well-tested.
- MCP route handlers (`handle_pre_invocation_hook`, guardrail engine, utilization scoring) are ENTIRELY untested.
- Integration tests must be added for the MCP layer, not just the backend.

---

## 3. Functional Requirements

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
