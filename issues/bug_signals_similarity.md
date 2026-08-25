---
labels: bug, agent-found
---

**File:** `mythrax-core/src/cognitive/pipeline/signals.rs`
**Line Numbers:** 96

**Description:**
In `cluster_facts`, when falling back to Jaccard similarity (intersection/union) because embeddings are invalid, the result is erroneously multiplied by `5.0`. This artificially inflates the similarity score, bypassing the threshold (`config.cluster_similarity`) and causing unrelated facts to be clustered together (false-positive clustering).

**Minimal Reproducible Scenario:**
1. Provide two facts with zeroed embeddings.
2. Their tokens have an intersection of 1 and a union of 10. Jaccard similarity is 0.1.
3. The code calculates `0.1 * 5.0 = 0.5`. If the threshold is 0.4, they are clustered despite minimal overlap.

**Severity:** Medium (pollutes context/clustering with unrelated facts)

**Suggested Fix:**
Remove the `* 5.0` multiplier and rely strictly on the mathematical definition of Jaccard similarity (intersection / union).
