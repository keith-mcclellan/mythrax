---
title: "Unbounded False-Positive Clustering"
labels: ["architecture-review", "adversarial"]
---

Finding -> Current Assumption -> Attack Scenario -> Blast Radius -> Recommended Structural Change

**Finding**: signals.rs clustering fallback erroneously multiplies Jaccard similarity by 5.0.
**Current Assumption**: Token intersection requires a massive artificial boost to be useful.
**Attack Scenario**: Generic tokens overlap slightly across disparate memories; the 5.0 multiplier inflates similarity to 1.0.
**Blast Radius**: The graph collapses into a single monolithic cluster, blowing up the LLM context window during cognitive synthesis.
**Recommended Structural Change**: Remove the hardcoded 5.0 multiplier and calibrate DBSCAN epsilon dynamically.

**Note:** Never close this issue without a documented architectural decision record (ADR) response.
