---
title: False-positive clustering due to Jaccard similarity multiplier
labels: bug, agent-found
---

**File:** `mythrax-core/src/cognitive/pipeline/signals.rs`
**Line Number:** 96

**Description:**
The agent clustering similarity fallback logic erroneously multiplies the Jaccard similarity (intersection/union) by 5.0, resulting in false-positive clustering. Jaccard similarity naturally falls between 0.0 and 1.0, but multiplying by 5.0 inflates the score massively, causing unrelated signals to be clustered together.

**Minimal Reproducible Scenario:**
Submit two minimally related signals that share a small fraction of overlapping terms (e.g., Jaccard similarity of 0.2). Due to the 5.0 multiplier, the final similarity score evaluates to 1.0, exceeding the clustering threshold and falsely grouping them.

**Severity:** Medium

**Suggested Fix:**
Remove the `* 5.0` multiplier from the fallback logic to preserve the standard Jaccard similarity bound of `[0.0, 1.0]`.
