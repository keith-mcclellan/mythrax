# Bug: Incorrect 5.0 multiplier in Jaccard similarity causes false-positive clustering

**File and Line Number**: `mythrax-core/src/cognitive/pipeline/signals.rs`, line 96

**Minimal Reproducible Scenario**:
Two independent agent memory facts share only 20% of their vocabulary (Jaccard similarity = 0.2). The fallback clustering logic multiplies this by 5.0: `(intersection as f32 / union as f32) * 5.0`. The resulting score is 1.0 (100%), which exceeds `config.cluster_similarity` and falsely groups unrelated facts into the same context window.

**Severity**: High

**Suggested Fix**:
Remove the `* 5.0` multiplier on line 96 so the Jaccard similarity properly outputs a normalized `[0.0, 1.0]` bounded value.

**Labels**: `bug`, `agent-found`