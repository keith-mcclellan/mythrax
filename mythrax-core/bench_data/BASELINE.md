# Mythrax LongMemEval *retrieval* Baseline (full 500)

**Metric:** LongMemEval *retrieval* (Recall@k / NDCG@k) — NOT QA accuracy.
**Dataset ID:** `xiaowu0162/longmemeval-cleaned`
**Pinned Revision (commit SHA):** `98d7416c24c778c2fee6e6f3006e7a073259d48f`
**Scored file:** `longmemeval_s_cleaned.json` (long-context haystack)
**Scored file SHA-256:** `d6f21ea9d60a0d56f34a05b609c79c88a451d2ae03597821ea3d5a9678c3a442`
**Split:** `full500` (official 500-question set, full longmemeval_s haystack)
**Mythrax Git Commit:** `bd9299f30cc45ba689068e2338f5d28465bc74a7`
**Evaluated at:** 2026-07-04T04:22:56.582420+00:00

## Aggregate Metrics
### Turn granularity (has_answer)
- **Recall_Any@5:** `0.8640`
- **Recall_All@5:** `0.5180`
- **nDCG@10:** `0.7915`
### Session granularity (answer_session_ids)
- **Recall_Any@5 (session):** `0.9460`
- **Recall_All@5 (session):** `0.6860`

## Per-Question-Type R@10 (turn recall_any)
- **knowledge-update** (n=88): R@10 = `0.8864`
- **multi-session** (n=112): R@10 = `0.8482`
- **single-session-assistant** (n=62): R@10 = `0.8710`
- **single-session-preference** (n=48): R@10 = `0.8125`
- **single-session-user** (n=78): R@10 = `0.8718`
- **temporal-reasoning** (n=112): R@10 = `0.8839`

> [!IMPORTANT]
> These are LongMemEval *retrieval* numbers scored over the full `longmemeval_s` haystack at the pinned revision above. Future optimizations must not regress `Recall_Any@5`. The `oracle` split is an upper-bound diagnostic only and is never published.
