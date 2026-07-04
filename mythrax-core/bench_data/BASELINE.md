# Mythrax LongMemEval *retrieval* Baseline (full 500)

**Metric:** LongMemEval *retrieval* (Recall@k / NDCG@k) — NOT QA accuracy.
**Dataset ID:** `xiaowu0162/longmemeval-cleaned`
**Pinned Revision (commit SHA):** `98d7416c24c778c2fee6e6f3006e7a073259d48f`
**Scored file:** `longmemeval_s_cleaned.json` (long-context haystack)
**Scored file SHA-256:** `d6f21ea9d60a0d56f34a05b609c79c88a451d2ae03597821ea3d5a9678c3a442`
**Split:** `full500` (official 500-question set, full longmemeval_s haystack)
**Mythrax Git Commit:** `4c5b096b0eb05bdddc59c60e05dd72e6d691b881`
**Evaluated at:** 2026-07-04T00:54:32.818758+00:00

## Aggregate Metrics
### Turn granularity (has_answer)
- **Recall_Any@5:** `0.6060`
- **Recall_All@5:** `0.3540`
- **nDCG@10:** `0.4273`
### Session granularity (answer_session_ids)
- **Recall_Any@5 (session):** `0.9540`
- **Recall_All@5 (session):** `0.7100`

## Per-Question-Type R@10 (turn recall_any)
- **knowledge-update** (n=78): R@10 = `0.8205`
- **multi-session** (n=133): R@10 = `0.6015`
- **single-session-assistant** (n=56): R@10 = `0.9107`
- **single-session-preference** (n=30): R@10 = `0.4667`
- **single-session-user** (n=70): R@10 = `0.7286`
- **temporal-reasoning** (n=133): R@10 = `0.6617`

> [!IMPORTANT]
> These are LongMemEval *retrieval* numbers scored over the full `longmemeval_s` haystack at the pinned revision above. Future optimizations must not regress `Recall_Any@5`. The `oracle` split is an upper-bound diagnostic only and is never published.
