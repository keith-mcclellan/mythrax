# Mythrax LongMemEval *retrieval* Baseline (full 500)

**Metric:** LongMemEval *retrieval* (Recall@k / NDCG@k) — NOT QA accuracy.
**Dataset ID:** `xiaowu0162/longmemeval-cleaned`
**Pinned Revision (commit SHA):** `98d7416c24c778c2fee6e6f3006e7a073259d48f`
**Scored file:** `longmemeval_s_cleaned.json` (long-context haystack)
**Scored file SHA-256:** `d6f21ea9d60a0d56f34a05b609c79c88a451d2ae03597821ea3d5a9678c3a442`
**Split:** `full500` (official 500-question set, full longmemeval_s haystack)
**Mythrax Git Commit:** `b3450fda2427ad390aabbc8f4474602d76911766`
**Evaluated at:** 2026-07-05T03:30:53.999192+00:00

## Aggregate Metrics
### Turn granularity (has_answer)
- **Recall_Any@5:** `0.8200`
- **Recall_All@5:** `0.5480`
- **nDCG@10:** `0.6247`
### Session granularity (answer_session_ids)
- **Recall_Any@5 (session):** `0.9680`
- **Recall_All@5 (session):** `0.7560`

## Per-Question-Type R@10 (turn recall_any)
- **knowledge-update** (n=78): R@10 = `0.9231`
- **multi-session** (n=133): R@10 = `0.8647`
- **single-session-assistant** (n=56): R@10 = `0.9821`
- **single-session-preference** (n=30): R@10 = `0.6667`
- **single-session-user** (n=70): R@10 = `0.9000`
- **temporal-reasoning** (n=133): R@10 = `0.9023`

> [!IMPORTANT]
> These are LongMemEval *retrieval* numbers scored over the full `longmemeval_s` haystack at the pinned revision above. Future optimizations must not regress `Recall_Any@5`. The `oracle` split is an upper-bound diagnostic only and is never published.
