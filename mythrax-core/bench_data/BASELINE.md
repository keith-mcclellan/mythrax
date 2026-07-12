# Mythrax LongMemEval *retrieval* Baseline (full 500)

**Metric:** LongMemEval *retrieval* (Recall@k / NDCG@k) — NOT QA accuracy.
**Dataset ID:** `xiaowu0162/longmemeval-cleaned`
**Pinned Revision (commit SHA):** `98d7416c24c778c2fee6e6f3006e7a073259d48f`
**Scored file:** `longmemeval_s_cleaned.json` (long-context haystack)
**Scored file SHA-256:** `d6f21ea9d60a0d56f34a05b609c79c88a451d2ae03597821ea3d5a9678c3a442`
**Split:** `full500` (official 500-question set, full longmemeval_s haystack)
**Mythrax Git Commit:** `6df06a2b37e59cedce4c16bc26e282846e57604a`
**Evaluated at:** 2026-07-12T17:44:40.376457+00:00

## Aggregate Metrics
### Turn granularity (has_answer)
- **Recall_Any@5:** `0.8800`
- **Recall_All@5:** `0.6660`
- **Recall_All@25:** `0.9260`
- **nDCG@10:** `0.7189`
### Session granularity (answer_session_ids)
- **Recall_Any@5 (session):** `1.0000`
- **Recall_All@5 (session):** `0.8740`

## Per-Question-Type R@10 (turn recall_any)
- **knowledge-update** (n=78): R@10 = `0.9231`
- **multi-session** (n=133): R@10 = `0.9098`
- **single-session-assistant** (n=56): R@10 = `1.0000`
- **single-session-preference** (n=30): R@10 = `0.9333`
- **single-session-user** (n=70): R@10 = `0.8857`
- **temporal-reasoning** (n=133): R@10 = `0.9549`

> [!IMPORTANT]
> These are LongMemEval *retrieval* numbers scored over the full `longmemeval_s` haystack at the pinned revision above. Future optimizations must not regress `Recall_Any@5`. The `oracle` split is an upper-bound diagnostic only and is never published.
