# Mythrax LongMemEval *retrieval* Baseline (full 500)

**Metric:** LongMemEval *retrieval* (Recall@k / NDCG@k) — NOT QA accuracy.
**Dataset ID:** `xiaowu0162/longmemeval-cleaned`
**Pinned Revision (commit SHA):** `98d7416c24c778c2fee6e6f3006e7a073259d48f`
**Scored file:** `longmemeval_s_cleaned.json` (long-context haystack)
**Scored file SHA-256:** `d6f21ea9d60a0d56f34a05b609c79c88a451d2ae03597821ea3d5a9678c3a442`
**Split:** `full500` (official 500-question set, full longmemeval_s haystack)
**Mythrax Git Commit:** `68a3397186ecd9fd83bcd695d6cec0d3b5b7aeec`
**Evaluated at:** 2026-06-29T03:38:26.993982+00:00

## Aggregate Metrics
### Turn granularity (has_answer)
- **Recall_Any@5:** `0.7380`
- **Recall_All@5:** `0.2800`
- **nDCG@10:** `0.4433`
### Session granularity (answer_session_ids)
- **Recall_Any@5 (session):** `0.8860`
- **Recall_All@5 (session):** `0.2980`

## Per-Question-Type R@10 (turn recall_any)
- **knowledge-update** (n=78): R@10 = `0.8846`
- **multi-session** (n=133): R@10 = `0.7820`
- **single-session-assistant** (n=56): R@10 = `1.0000`
- **single-session-preference** (n=30): R@10 = `0.6000`
- **single-session-user** (n=70): R@10 = `0.8143`
- **temporal-reasoning** (n=133): R@10 = `0.7970`

> [!IMPORTANT]
> These are LongMemEval *retrieval* numbers scored over the full `longmemeval_s` haystack at the pinned revision above. Future optimizations must not regress `Recall_Any@5`. The `oracle` split is an upper-bound diagnostic only and is never published.
