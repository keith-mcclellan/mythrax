# Mythrax LongMemEval *retrieval* Baseline (full 500)

**Metric:** LongMemEval *retrieval* (Recall@k / NDCG@k) — NOT QA accuracy.
**Dataset ID:** `xiaowu0162/longmemeval-cleaned`
**Pinned Revision (commit SHA):** `98d7416c24c778c2fee6e6f3006e7a073259d48f`
**Scored file:** `longmemeval_s_cleaned.json` (long-context haystack)
**Scored file SHA-256:** `d6f21ea9d60a0d56f34a05b609c79c88a451d2ae03597821ea3d5a9678c3a442`
**Split:** `full500` (official 500-question set, full longmemeval_s haystack)
**Mythrax Git Commit:** `e66bf4aa6f579691fbf7007f2a50c1cb6a7b8c52`
**Evaluated at:** 2026-06-30T02:30:36.081689+00:00

## Aggregate Metrics
### Turn granularity (has_answer)
- **Recall_Any@5:** `0.2280`
- **Recall_All@5:** `0.1200`
- **nDCG@10:** `0.1750`
### Session granularity (answer_session_ids)
- **Recall_Any@5 (session):** `0.3460`
- **Recall_All@5 (session):** `0.1620`

## Per-Question-Type R@10 (turn recall_any)
- **knowledge-update** (n=78): R@10 = `0.3718`
- **multi-session** (n=133): R@10 = `0.2105`
- **single-session-assistant** (n=56): R@10 = `0.8393`
- **single-session-preference** (n=30): R@10 = `0.0667`
- **single-session-user** (n=70): R@10 = `0.2000`
- **temporal-reasoning** (n=133): R@10 = `0.1579`

> [!IMPORTANT]
> These are LongMemEval *retrieval* numbers scored over the full `longmemeval_s` haystack at the pinned revision above. Future optimizations must not regress `Recall_Any@5`. The `oracle` split is an upper-bound diagnostic only and is never published.
