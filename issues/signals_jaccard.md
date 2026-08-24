**Title:** Jaccard similarity fallback logic error
**Labels:** bug, agent-found
**File:** mythrax-core/src/cognitive/pipeline/signals.rs
**Line:** 96
**Severity:** Medium
**Minimal Reproducible Scenario:** The agent clustering similarity fallback logic erroneously multiplies the Jaccard similarity (intersection/union) by 5.0, resulting in similarity scores exceeding 1.0 and causing false-positive clustering.
**Suggested Fix:** Remove the `* 5.0` multiplier from the Jaccard similarity calculation.
