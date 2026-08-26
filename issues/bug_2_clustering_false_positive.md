## Title: False-positive clustering in agent signals due to 5.0 multiplier on Jaccard similarity


**Labels:** `bug`, `agent-found`

### Description
In `mythrax-core/src/cognitive/pipeline/signals.rs`, within `cluster_facts`, if embeddings are not valid, it falls back to a Jaccard similarity based on tokens. However, it erroneously multiplies the result (intersection/union) by 5.0. Since Jaccard similarity is between 0 and 1, multiplying by 5.0 will cause the similarity to easily exceed the configured threshold, resulting in false-positive clustering of unassociated facts.

**File:** `mythrax-core/src/cognitive/pipeline/signals.rs`
**Line:** 86

### Minimal Reproducible Scenario
Have two facts with a Jaccard similarity of 0.2 and a clustering threshold of 0.8. The calculated similarity will be `0.2 * 5.0 = 1.0`, exceeding the threshold, and they will be erroneously clustered together.

### Severity
High - False positives in cognitive pipeline clustering degrade agent reasoning and pollute the generated ideas.

### Suggested Fix
Remove the `* 5.0` multiplier from the Jaccard similarity fallback calculation:
`(intersection as f32 / union as f32)` instead of `(intersection as f32 / union as f32) * 5.0`
