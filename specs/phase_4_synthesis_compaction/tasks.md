# Tasks: Phase 4 (v0.5.0) — Advanced Synthesis & Compaction

## T1: Semantic Insight Compaction Refactoring
- **Purpose**: Refactor compactor scope clustering to use semantic grouping instead of sequential chunks.
- **Related Requirements**: Outcome 1, AC 1
- **Related Tests**: `test_dbscan_insight_compaction`
- **Inputs**: `compactor.rs`
- **Actions**:
  - Load embeddings for all scope insights by querying their database records.
  - Call `dbscan` to cluster them.
  - Summarize each cluster separately, and summarize outliers together in a fallback compaction.
- **Expected Output**: Clusters grouped semantically.
- **Validation**: `cargo test`

## T2: Centroid Drift & Split Management
- **Purpose**: Prevent insights from bloating by splitting them when semantic drift is detected.
- **Related Requirements**: Outcome 2, AC 2
- **Related Tests**: `test_insight_centroid_drift_split`
- **Inputs**: `synthesis.rs`
- **Actions**:
  - Compute pairwise cosine distance for all source episodes of an insight.
  - If maximum distance exceeds `0.30`, run DBSCAN to divide the episodes.
  - Write new split insights, relates_to edges, and delete the original insight.
- **Expected Output**: Bloated insights are cleanly split.
- **Validation**: `cargo test`

## T3: Wisdom Rule Deduplication
- **Purpose**: Deduplicate dynamic wisdom rules.
- **Related Requirements**: Outcome 3, AC 3
- **Related Tests**: `test_wisdom_rule_deduplication_dynamic`, `test_wisdom_rule_deduplication_skills_anchor`
- **Inputs**: `synthesis.rs`
- **Actions**:
  - Compare new rules against existing rules using embedding similarity.
  - Generalize similar dynamic rules.
  - Link episodes to the existing rules if matched with a skills rule anchor.
- **Expected Output**: Dynamic rule duplication eliminated.
- **Validation**: `cargo test`

## T4: Targeted Cross-Skill Harvesting
- **Purpose**: Optimize cross-skill conflict analysis using description clustering.
- **Related Requirements**: Outcome 4, AC 4
- **Related Tests**: `test_targeted_cross_skill_harvesting`
- **Inputs**: `harvest.rs`
- **Actions**:
  - Embed skill descriptions and cluster them using DBSCAN.
  - Run conflict analysis only on clusters of size >= 2.
- **Expected Output**: Lower token usage and highly focused conflict analysis.
- **Validation**: `cargo test`

## T5: Full Verification & Walkthrough
- **Purpose**: Run all tests and document changes.
- **Related Requirements**: All
- **Related Tests**: All
- **Actions**:
  - Run `cargo test` across all targets.
  - Rebuild release binary.
  - Write `walkthrough.md`.
- **Expected Output**: 100% test pass rate.
- **Validation**: All tests pass.
