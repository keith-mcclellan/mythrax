# Mythrax Benchmarks

This file tracks the retrieval performance of the Mythrax Memory System releases.

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
