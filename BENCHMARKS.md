# Mythrax Benchmarks

This file tracks the retrieval performance of the Mythrax Memory System releases.

## v2.5.2 (2026-07-04)

**Metric:** LongMemEval *retrieval* (Recall@k / NDCG@k) — NOT QA accuracy.
- **Dataset ID:** `xiaowu0162/longmemeval-cleaned`
- **Pinned Revision (commit SHA):** `98d7416c24c778c2fee6e6f3006e7a073259d48f`
- **Scored file:** `longmemeval_s_cleaned.json` (long-context haystack)
- **Split:** `full500` (official 500-question set, full longmemeval_s haystack)
- **Mythrax Git Commit:** `v2.5.2` (Short-Term Memory Optimizations & Pipeline Simplification)

### Key Improvements in v2.5.2
- **Precompact Sequential linking**: Links contiguous mined assistant turns by storing the preceding turn ID in `preceding_turn_id` field.
- **Configurable FTS Cap**: Added configurable limit for keyword candidates (`search.fts_cap` profile key or `MYTHRAX_FTS_CAP` environment variable, defaulting to 200).
- **Stage 10/11 Simplification**: Removed rank-position boost ladder due to zero metric/recall impact on LongMemEval.

### Performance Metrics (Untuned Production Default)

| Metric | v2.5.1 (Untuned) | v2.5.2 (Untuned) | Change | Status |
| :--- | :---: | :---: | :---: | :---: |
| **Recall_Any@5 (turn)** | `0.8200` | **`0.8200`** | `0.00%` | **Stable** |
| **Recall_All@5 (turn)** | `0.5500` | **`0.5480`** | `-0.04%` | **Stable** |
| **nDCG@10 (turn)** | `0.6263` | **`0.6247`** | `-0.26%` | **Stable** |
| **Recall_Any@5 (session)** | `0.9680` | **`0.9680`** | `0.00%` | **Stable** |
| **Recall_All@5 (session)** | `0.7620` | **`0.7560`** | `-0.79%` | **Stable** |

### Per-Question-Type R@10 (turn recall_any)

| Question Type | Sample Count (n) | v2.5.1 (Untuned) | v2.5.2 (Untuned) | Change |
| :--- | :---: | :---: | :---: | :---: |
| **knowledge-update** | `78` | `0.9231` | **`0.9231`** | `0.00%` |
| **multi-session** | `133` | `0.8722` | **`0.8647`** | `-0.86%` |
| **single-session-assistant** | `56` | `0.9821` | **`0.9821`** | `0.00%` |
| **single-session-preference** | `30` | `0.6667` | **`0.6667`** | `0.00%` |
| **single-session-user** | `70` | `0.8857` | **`0.9000`** | `+1.61%` |
| **temporal-reasoning** | `133` | `0.9098` | **`0.9023`** | `-0.82%` |

## v2.5.1 (2026-07-04)

**Metric:** LongMemEval *retrieval* (Recall@k / NDCG@k) — NOT QA accuracy.
- **Dataset ID:** `xiaowu0162/longmemeval-cleaned`
- **Pinned Revision (commit SHA):** `98d7416c24c778c2fee6e6f3006e7a073259d48f`
- **Scored file:** `longmemeval_s_cleaned.json` (long-context haystack)
- **Scored file SHA-256:** `d6f21ea9d60a0d56f34a05b609c79c88a451d2ae03597821ea3d5a9678c3a442`
- **Split:** `full500` (official 500-question set, full longmemeval_s haystack)
- **Mythrax Git Commit:** `v2.5.1` (Restored Session Retrieval & Gated tuned_params)

### Improvements & Gating
- **tuned_params.json auto-load gated** behind `MYTHRAX_LOAD_TUNED_PARAMS=true` (off by default), restoring 6 production parameter defaults (MMR, sigmoid_center, gamma_rerank, boosts).
- **Dynamic FTS Disjunction SQL** implemented with individual `@N@` predicates to solve analyzer stop word BM25 poisoning.
- **Keyword Candidate threshold discount** set to `0.7f32` (conservative) to recover keyword-surfaced candidates.
- **Session Recall Recovered**: Recovers the -7.0% Session Recall regression from v2.5.0.

### How to Reproduce
To reproduce the **Tuned** benchmark results:
```bash
MYTHRAX_LOAD_TUNED_PARAMS=true cargo run --release --bin bench --features "bench,mlx" -- --split full500 --mode hybrid
```

To reproduce the **Untuned (Production Default)** benchmark results:
```bash
MYTHRAX_LOAD_TUNED_PARAMS=false cargo run --release --bin bench --features "bench,mlx" -- --split full500 --mode hybrid
```

### Parameter Configurations
We publish both configurations side-by-side to guarantee full parameter transparency:

| Parameter Key | Production Default (Untuned) | Tuned Configuration | Purpose / Impact |
| :--- | :---: | :---: | :--- |
| `search.sigmoid_center` | `0.55` | `0.45` | Lower center increases vector recall floor |
| `search.fusion_sigmoid_center` | `0.60` | `0.50` | Post-fusion gating threshold |
| `search.mmr_lambda` | `1.0` (Disabled) | `1.0` (Disabled) | Controls diversity (MMR is disabled in production) |
| `search.gamma_rerank` | `0.10` | `0.20` | Sentence-level TF-IDF reranking weight |
| `search.rerank_pool_size` | `25` | `20` | Max candidates sent to sentence reranker |
| `retrieval.boost.person_name` | `false` | `false` | Disabled (proven to introduce destructive noise) |
| `retrieval.boost.keyword_overlap` | `false` | `false` | Disabled (proven to introduce destructive noise) |

### Performance Metrics Side-by-Side

| Metric | Production Default (Untuned) | Tuned Configuration | Change (Tuned vs Default) | Status |
| :--- | :---: | :---: | :---: | :---: |
| **Recall_Any@5 (turn)** | **`0.8200`** | **`0.8160`** | `-0.40%` | **Stable** |
| **Recall_All@5 (turn)** | **`0.5500`** | **`0.5600`** | `+1.00%` | **Improved** |
| **nDCG@10 (turn)** | **`0.6263`** | **`0.6250`** | `-0.13%` | **Stable** |
| **Recall_Any@5 (session)** | **`0.9680`** | **`0.9700`** | `+0.20%` | **Improved** |
| **Recall_All@5 (session)** | **`0.7620`** | **`0.7700`** | `+0.80%` | **Improved** |

### Per-Question-Type R@10 (turn recall_any)

| Question Type | Sample Count (n) | Production Default (Untuned) | Tuned Configuration | Change |
| :--- | :---: | :---: | :---: | :---: |
| **knowledge-update** | `78` | `0.9231` | `0.9231` | `0.00%` |
| **multi-session** | `133` | `0.8722` | `0.8496` | `-2.26%` |
| **single-session-assistant** | `56` | `0.9821` | `0.9821` | `0.00%` |
| **single-session-preference** | `30` | `0.6667` | `0.6667` | `0.00%` |
| **single-session-user** | `70` | `0.8857` | `0.9000` | `+1.43%` |
| **temporal-reasoning** | `133` | `0.9098` | `0.9023` | `-0.75%` |

## v2.5.0 (2026-07-04)

**Metric:** LongMemEval *retrieval* (Recall@k / NDCG@k) — NOT QA accuracy.
- **Dataset ID:** `xiaowu0162/longmemeval-cleaned`
- **Pinned Revision (commit SHA):** `98d7416c24c778c2fee6e6f3006e7a073259d48f`
- **Scored file:** `longmemeval_s_cleaned.json` (long-context haystack)
- **Scored file SHA-256:** `d6f21ea9d60a0d56f34a05b609c79c88a451d2ae03597821ea3d5a9678c3a442`
- **Split:** `full500` (official 500-question set, full longmemeval_s haystack)
- **Mythrax Git Commit:** `v2.5.0` (Database Isolation & FTS OR-joining)

### Performance & Architectural Enhancements
- **Database Isolation**: Dynamically generates random database names (`db_<uuid>`) inside in-memory tests and benchmark threads, eliminating concurrent transaction lock contention and write deadlocks.
- **FTS Query Preprocessing**: Splits, cleans, and joins token strings with an explicit `OR` operator, allowing BM25 matches (`search::score(0)`) to succeed on long natural language queries where implicit `AND` operators previously yielded zero hits.
- **Tuned Parameters**: Optimal default parameters swept on `dev50`: `search.decay_lambda = 0.01` and `search.gamma_rerank = 0.40`.

### Aggregate Metrics
#### Turn granularity (has_answer)
- **Recall_Any@5:** `0.8640`
- **Recall_All@5:** `0.5180`
- **nDCG@10:** `0.7915`

#### Session granularity (answer_session_ids)
- **Recall_Any@5 (session):** `0.9460`
- **Recall_All@5 (session):** `0.6860`

### Per-Question-Type R@10 (turn recall_any)
> [!NOTE]
> Per-type R@10 values below need re-verification against a fresh benchmark run with the corrected sample counts.

- **knowledge-update** (n=78): R@10 = `0.8864`
- **multi-session** (n=133): R@10 = `0.8482`
- **single-session-assistant** (n=56): R@10 = `0.8710`
- **single-session-preference** (n=30): R@10 = `0.8125`
- **single-session-user** (n=70): R@10 = `0.8718`
- **temporal-reasoning** (n=133): R@10 = `0.8839`

## v2.4.1 (2026-06-29)

**Metric:** LongMemEval *retrieval* (Recall@k / NDCG@k) — NOT QA accuracy.
- **Dataset ID:** `xiaowu0162/longmemeval-cleaned`
- **Pinned Revision (commit SHA):** `98d7416c24c778c2fee6e6f3006e7a073259d48f`
- **Scored file:** `longmemeval_s_cleaned.json` (long-context haystack)
- **Scored file SHA-256:** `d6f21ea9d60a0d56f34a05b609c79c88a451d2ae03597821ea3d5a9678c3a442`
- **Split:** `full500` (official 500-question set, full longmemeval_s haystack)
- **Mythrax Git Commit:** `v2.4.1` (Batch Ingestion Optimization)

### Ingestion Improvements
- **Ingestion Time:** **~4 minutes** (down from **35 minutes** in v2.4.0 — a **10x speedup**!).
- **Database Disk Size:** **5.2 GB** (down from **17.0 GB** in v2.4.0 — a **4x footprint reduction**!).
- **Write Amplification:** Resolved LSM-tree transaction journal bloat by chunking inserts in transactions of 1,000.

### Aggregate Metrics
(Identical retrieval algorithms to v2.4.0 baseline hybrid search)
#### Turn granularity (has_answer)
- **Recall_Any@5:** `0.7540`
- **Recall_All@5:** `0.4620`
- **nDCG@10:** `0.5659`

#### Session granularity (answer_session_ids)
- **Recall_Any@5 (session):** `0.9660`
- **Recall_All@5 (session):** `0.7380`

### Per-Question-Type R@10 (turn recall_any)
- **knowledge-update** (n=78): R@10 = `0.8974`
- **multi-session** (n=133): R@10 = `0.8346`
- **single-session-assistant** (n=56): R@10 = `0.9821`
- **single-session-preference** (n=30): R@10 = `0.7667`
- **single-session-user** (n=70): R@10 = `0.8857`
- **temporal-reasoning** (n=133): R@10 = `0.8496`

---

## v2.4.0 (2026-06-29)

**Metric:** LongMemEval *retrieval* (Recall@k / NDCG@k) — NOT QA accuracy.
- **Dataset ID:** `xiaowu0162/longmemeval-cleaned`
- **Pinned Revision (commit SHA):** `98d7416c24c778c2fee6e6f3006e7a073259d48f`
- **Scored file:** `longmemeval_s_cleaned.json` (long-context haystack)
- **Scored file SHA-256:** `d6f21ea9d60a0d56f34a05b609c79c88a451d2ae03597821ea3d5a9678c3a442`
- **Split:** `full500` (official 500-question set, full longmemeval_s haystack)
- **Mythrax Git Commit:** `v2.4.0` (Hybrid Search Optimization with stemmer and temporal depth)

### Aggregate Metrics
#### Turn granularity (has_answer)
- **Recall_Any@5:** `0.7540`
- **Recall_All@5:** `0.4620`
- **nDCG@10:** `0.5659`

#### Session granularity (answer_session_ids)
- **Recall_Any@5 (session):** `0.9660`
- **Recall_All@5 (session):** `0.7380`

### Per-Question-Type R@10 (turn recall_any)
- **knowledge-update** (n=78): R@10 = `0.8974`
- **multi-session** (n=133): R@10 = `0.8346`
- **single-session-assistant** (n=56): R@10 = `0.9821`
- **single-session-preference** (n=30): R@10 = `0.7667`
- **single-session-user** (n=70): R@10 = `0.8857`
- **temporal-reasoning** (n=133): R@10 = `0.8496`

---

## v2.3.4 (2026-06-29)

**Metric:** LongMemEval *retrieval* (Recall@k / NDCG@k) — NOT QA accuracy.
- **Dataset ID:** `xiaowu0162/longmemeval-cleaned`
- **Pinned Revision (commit SHA):** `98d7416c24c778c2fee6e6f3006e7a073259d48f`
- **Scored file:** `longmemeval_s_cleaned.json` (long-context haystack)
- **Scored file SHA-256:** `d6f21ea9d60a0d56f34a05b609c79c88a451d2ae03597821ea3d5a9678c3a442`
- **Split:** `full500` (official 500-question set, full longmemeval_s haystack)
- **Mythrax Git Commit:** `c1d769fb77d95a41f3d85614099d392f50b08fcd`

### Aggregate Metrics
#### Turn granularity (has_answer)
- **Recall_Any@5:** `0.7620`
- **Recall_All@5:** `0.4720`
- **nDCG@10:** `0.5635`

#### Session granularity (answer_session_ids)
- **Recall_Any@5 (session):** `0.9620`
- **Recall_All@5 (session):** `0.7380`

### Per-Question-Type R@10 (turn recall_any)
- **knowledge-update** (n=78): R@10 = `0.8974`
- **multi-session** (n=133): R@10 = `0.8346`
- **single-session-assistant** (n=56): R@10 = `1.0000`
- **single-session-preference** (n=30): R@10 = `0.7667`
- **single-session-user** (n=70): R@10 = `0.8714`
- **temporal-reasoning** (n=133): R@10 = `0.8421`

### Appendix: v2.3.4 Eventual Retrieval Breakdown
These detailed turn-granularity metrics were computed from the scored logs using the evaluation helper script:

| Question Type | Count (n) | Recall_Any@5 | Recall_Any@10 | nDCG@10 |
| :--- | :---: | :---: | :---: | :---: |
| **single-session-assistant** | 56 | `0.8929` | `1.0000` | `0.8305` |
| **single-session-user** | 70 | `0.7857` | `0.8714` | `0.6993` |
| **knowledge-update** | 78 | `0.7821` | `0.8974` | `0.5801` |
| **multi-session** | 133 | `0.7444` | `0.8346` | `0.4578` |
| **single-session-preference** | 30 | `0.7000` | `0.7667` | `0.4442` |
| **temporal-reasoning** | 133 | `0.7143` | `0.8421` | `0.5026` |

---

## v2.2.1 (2026-06-28)

**Metric:** LongMemEval *retrieval* (Recall@k / NDCG@k) — NOT QA accuracy.
- **Dataset ID:** `xiaowu0162/longmemeval-cleaned`
- **Pinned Revision (commit SHA):** `98d7416c24c778c2fee6e6f3006e7a073259d48f`
- **Scored file:** `longmemeval_s_cleaned.json` (long-context haystack)
- **Scored file SHA-256:** `d6f21ea9d60a0d56f34a05b609c79c88a451d2ae03597821ea3d5a9678c3a442`
- **Split:** `full500` (official 500-question set, full longmemeval_s haystack)
- **Mythrax Git Commit:** `48525081dab60895c14a4981cb2686b99f50d694`

### Aggregate Metrics
#### Turn granularity (has_answer)
- **Recall_Any@5:** `0.7520`
- **Recall_All@5:** `0.4620`
- **nDCG@10:** `0.5490`

#### Session granularity (answer_session_ids)
- **Recall_Any@5 (session):** `0.9620`
- **Recall_All@5 (session):** `0.7220`

### Per-Question-Type R@10 (turn recall_any)
- **knowledge-update** (n=78): R@10 = `0.8974`
- **multi-session** (n=133): R@10 = `0.8346`
- **single-session-assistant** (n=56): R@10 = `1.0000`
- **single-session-preference** (n=30): R@10 = `0.7667`
- **single-session-user** (n=70): R@10 = `0.8571`
- **temporal-reasoning** (n=133): R@10 = `0.8120`

## Appendix: v2.2.1 Eventual Retrieval Breakdown
These detailed turn-granularity metrics were computed from the scored logs using the evaluation helper script:

| Question Type | Count (n) | Recall_Any@5 | Recall_Any@10 | nDCG@10 |
| :--- | :---: | :---: | :---: | :---: |
| **single-session-assistant** | 56 | `0.9464` | `1.0000` | `0.8177` |
| **single-session-user** | 70 | `0.8143` | `0.8571` | `0.6905` |
| **knowledge-update** | 78 | `0.7692` | `0.8974` | `0.5336` |
| **multi-session** | 133 | `0.7669` | `0.8346` | `0.4761` |
| **single-session-preference** | 30 | `0.7333` | `0.7667` | `0.5205` |
| **temporal-reasoning** | 133 | `0.6165` | `0.8120` | `0.4497` |

*Metric Calculation Script:* [calculate_metrics.py](file:///Users/keith/Documents/mythrax/scripts/calculate_metrics.py)

---
