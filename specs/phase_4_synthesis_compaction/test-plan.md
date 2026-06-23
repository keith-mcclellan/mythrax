# Test Plan: Phase 4 (v0.5.0) — Advanced Synthesis & Compaction

This test plan defines the unit, integration, and regression tests to verify Phase 4 features.

---

## Unit & Integration Tests

### 1. Semantic Insight Compaction
- **`test_dbscan_insight_compaction`**
  - **Purpose**: Verify that the `Compactor` groups insights using DBSCAN and routes outliers into a single fallback miscellaneous compaction.
  - **Inputs**: 4 mock insights: 3 highly similar (Cluster A) and 1 distinct (Outlier).
  - **Expected Outcome**: Generates 2 compaction files: one summarizing Cluster A and one miscellaneous compaction summarizing the outlier.

### 2. Centroid Drift & Split Management
- **`test_insight_centroid_drift_split`**
  - **Purpose**: Verify that when maximum pairwise distance of episodes inside an insight exceeds `0.30`, the insight is split.
  - **Inputs**: An insight with 4 source episodes. 2 episodes are highly similar to each other, and the other 2 are similar to each other but very distinct from the first pair (causing max pairwise distance > 0.30).
  - **Expected Outcome**: The original insight file is deleted, and 2 new insight files representing the split clusters are created and linked to their respective source episodes in SurrealDB.

### 3. Wisdom Rule Deduplication & Generalization
- **`test_wisdom_rule_deduplication_dynamic`**
  - **Purpose**: Verify that similar dynamic rules (> 0.80 similarity) are merged and generalized.
  - **Inputs**: 2 similar dynamic rules created.
  - **Expected Outcome**: The LLM generalizes them, saves the new rule, deletes the old ones from the filesystem and database, and relates the combined set of episodes to the new rule.

- **`test_wisdom_rule_deduplication_skills_anchor`**
  - **Purpose**: Verify that `tier: "skills"` rules act as read-only anchors and are not modified during deduplication.
  - **Inputs**: A `tier: "skills"` rule and a similar new rule generated.
  - **Expected Outcome**: The new rule is discarded (not saved), and its episodes are related directly to the existing skills rule.

### 4. Targeted Cross-Skill Harvesting
- **`test_targeted_cross_skill_harvesting`**
  - **Purpose**: Verify that skills are clustered by description and only groups of size >= 2 are conflict-analyzed.
  - **Inputs**: 5 skills: 3 in Cluster A, 2 in Cluster B, and 1 outlier.
  - **Expected Outcome**: Executes cross-skill conflict completions for Cluster A and Cluster B, but skips the outlier skill completely.

---

## Verification Commands
`cargo test`
