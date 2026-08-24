---
title: Erroneous 5.0 multiplier in Jaccard similarity fallback causes false-positive clustering
labels: bug, agent-found
---
**File and Line:** `mythrax-core/src/cognitive/pipeline/signals.rs`, line 96

**Minimal Reproducible Scenario:** When calculating similarity between fact tokens, the code performs the calculation `(intersection as f32 / union as f32) * 5.0`. By multiplying the Jaccard similarity by 5.0, a true Jaccard similarity of 0.2 (20%) is artificially boosted to 1.0 (100%), which silently succeeds but exceeds the clustering threshold and incorrectly groups unrelated facts together.

**Severity:** Medium

**Suggested Fix:** Remove the `* 5.0` multiplier from the fallback calculation and rely on the actual Jaccard similarity ratio (between 0.0 and 1.0).
