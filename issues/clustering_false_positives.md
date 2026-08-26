---
labels: bug, agent-found
severity: Medium
---

# Agent clustering similarity fallback logic false positives

**File:** `mythrax-core/src/cognitive/pipeline/signals.rs`
**Line:** 96

## Description
The agent clustering similarity fallback logic erroneously multiplies the Jaccard similarity (intersection/union) by 5.0. Since Jaccard similarity is bounded between 0.0 and 1.0, multiplying by 5.0 artificially inflates the score up to 5.0, resulting in false-positive clustering where totally unrelated facts are grouped together if they share even a few tokens.

## Minimal Reproducible Scenario
1. Submit two facts with no valid embeddings (fallback is triggered).
2. The first fact has tokens {"agent", "task", "start"}.
3. The second fact has tokens {"agent", "error", "stop"}.
4. Intersection is 1 ("agent"), Union is 5.
5. Base Jaccard similarity = 1/5 = 0.2.
6. The code multiplies by 5.0 -> Score becomes 1.0.
7. This perfectly triggers the similarity threshold, falsely clustering them as identical concepts.

## Suggested Fix
Remove the `* 5.0` multiplier from the fallback Jaccard calculation on line 96. The similarity should remain a pure ratio between 0.0 and 1.0: `(intersection as f32 / union as f32)`.
