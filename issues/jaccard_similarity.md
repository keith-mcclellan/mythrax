Title: Agent clustering similarity fallback erroneously multiplies Jaccard similarity by 5.0
Labels: bug, agent-found

File: mythrax-core/src/cognitive/pipeline/signals.rs, Line: 96
Severity: High
Scenario: In `cluster_facts`, when vector embeddings are invalid, a fallback Jaccard similarity is computed. The logic multiplies the result by 5.0 `(intersection as f32 / union as f32) * 5.0`, scaling the max similarity to 5.0 and resulting in false-positive clustering when checked against `config.cluster_similarity`.
Suggested Fix: Remove the `* 5.0` multiplier and ensure Jaccard similarity stays bounded between 0.0 and 1.0.
