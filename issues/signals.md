## Bug Report: Erroneous 5.0 multiplier in cluster similarity fallback

**File:** `mythrax-core/src/cognitive/pipeline/signals.rs`
**Line:** 96

**Minimal Reproducible Scenario:**
When clustering agents or facts based on token overlap (Jaccard similarity fallback logic), the ratio of intersection over union is multiplied by 5.0: `(intersection as f32 / union as f32) * 5.0`. Since Jaccard similarity is typically bounded between 0.0 and 1.0, multiplying by 5.0 breaks the probability space and leads to massive false positives, causing disparate agents or facts to be erroneously clustered together.

**Severity:** Medium (Logic Error/False Positives)

**Suggested Fix:**
Remove the `* 5.0` multiplier and rely strictly on the mathematical definition of Jaccard similarity.
