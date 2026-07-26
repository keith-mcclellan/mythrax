# Specification: Arbor Framework Alignment & Single-Pass Chunked Ingestion

## 1. Overview
Align Mythrax core memory architecture with the Arbor framework research paper (`arXiv:2606.11926v1.pdf`) and resolve performance bottlenecks during high-throughput transcript ingestion.

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
- Batch inverted index IDF updates into set-based SQL queries (`UPDATE idf_index ...`) eliminating sequential 1,000-query loops.
- Suppress filesystem watcher (`watcher.rs`) sync when `IS_INGESTING == true` to prevent RocksDB write lock collisions.

### C. Upward Insight Propagation (`TreePropagate`)
- Traverse parent-child graph edges (`parent_id` / `relates_to`) during scope compaction.
- Abstract child leaf insights into parent scope nodes: $\iota_{\text{parent}} \leftarrow \text{Abstract}(\{\iota_{\text{children}}\})$.
- Export accumulated `Negative Constraints` into root scope compaction notes.

### D. Negative Constraint Ideation Gating (`TreeView(format="constraints")`)
- Aggregate negative lessons from failed/pruned nodes into explicit constraint blocks that condition Cloud Brain dreaming and ideation.

### E. Convergence Detection & Parent Exhaustion
- Monitor score velocity over a sliding window ($\Delta \text{score} / \Delta \text{visits}$).
- Escalate non-improving cycles (`warn`, `paradigm_shift`, `stop`) and flag parent exhaustion to trigger fresh depth-1 exploration branches.

### F. Held-Out Validation Gate (`Etest` / Merge Gate)
- Validate newly distilled project insights and forged rules against held-out verification criteria (`TestCommandEvaluator` / `LlmCriticEvaluator`) in an isolated worktree before promoting to global cross-project `wisdom`.

---

## 3. Non-Functional Requirements
- **Performance**: Ingest 1,000+ conversation turns into SurrealDB and Markdown files in under 5 seconds.
- **Integrity**: Zero loss of raw compiler error tracebacks during summarization.
- **Safety**: Zero RocksDB lock panics or SurrealDB transaction timeouts.
