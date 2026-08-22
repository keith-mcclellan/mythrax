## Bug Report: Agent Clustering Similarity Score Anomaly

**File & Line:** `mythrax-core/src/cognitive/pipeline/signals.rs`, line 96 (`(intersection as f32 / union as f32) * 5.0`)

**Minimal Reproducible Scenario:** Trigger a pipeline run where two facts do not have valid embeddings. The code calculates Jaccard similarity (0.0 to 1.0) but multiplies it by `5.0`. A 20% word overlap receives a score of `1.0`, guaranteeing false-positive clustering.

**Severity:** High (Agent Memory Corruption).

**Suggested Fix:** Remove the `* 5.0` multiplier so Jaccard similarity correctly returns a value between 0.0 and 1.0.
