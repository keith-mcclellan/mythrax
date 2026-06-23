# Clarify: Phase 4 (v0.5.0) — Advanced Synthesis & Compaction

## Restated Request
Implement the Phase 4 (v0.5.0) features of the Mythrax Cognitive Pipeline:
1. **4.1 Semantic Project Briefs (Similarity Clustering)**: Group scope-specific insights using DBSCAN before passing them to the LLM to generate Compactions (Project Briefs).
2. **4.2 Centroid Drift & Split Management (Dreaming)**: Monitor pairwise cosine distance of episodes within an Insight. If drift exceeds a threshold, execute a subdivision split.
3. **4.3 Wisdom Rule Deduplication & Generalization**: Compare wisdom rules using embedding similarity. Generalize/merge rules if similarity > 0.80. Treat `tier: "skills"` rules as read-only, immutable anchors.
4. **4.4 Targeted Cross-Skill Harvesting**: Cluster skills by description similarity using DBSCAN and execute cross-skill conflict analysis only on related clusters.

## Known Facts
- **Custom DBSCAN**: Inside [synthesis.rs](file:///Users/keith/Documents/self-improvement-engine/mythrax-core/src/cognitive/synthesis.rs#L17), we have a custom `dbscan` implementation and `cosine_distance` metric.
- **Embedded Database**: RocksDB is single-process and locks.
- **Active Model**: The newly configured local model `mlx-community/Qwen3.6-35B-A3B-4bit` runs on port 8080.
- **Rule Tiers**: Rules have tiers: `"skills"`, `"forge"`, `"dynamic"`. Precedence is: `skills` > `dynamic`/`forge`.

## Assumptions
1. **Compactor Settings**: We can reuse the default dreaming `eps` (e.g. `0.10`) and `min_samples = 2` (or custom parameters) for DBSCAN clustering of insights.
2. **Pairwise Distance Threshold**: An Insight drift subdivision split should be triggered if the maximum pairwise cosine distance between any two episodes in the insight's `source_episodes` exceeds `0.30` (or another threshold like `0.35`).
3. **Splitting Procedure**: To split a drifting insight, we will run DBSCAN (`eps = 0.08`, `min_samples = 2`) on the embeddings of its source episodes. If multiple clusters are formed, we split the insight into corresponding new insights (generating new titles, summaries, and Markdown files) and archiving/deleting the old drifting insight.
4. **Wisdom Generalization**: Merging rules involves generating a new generalized rule via an LLM completion and updating the database. The new rule will inherit the combined set of `source_episodes`.
5. **Immutable Skills Rules**: If a new rule is highly similar (`> 0.80` similarity) to a `tier: "skills"` rule, the new rule is considered redundant. We discard/skip saving the new rule, and link its source episodes directly to the existing skills rule via `relates_to` edges.

## Ambiguities & Tradeoffs
- **Outlier Insights in Compaction**: If an insight is categorized as an outlier by DBSCAN (i.e. label is `None`), should it be ignored during compaction, or should we group all outliers into a fallback "Miscellaneous" chunk for summary?
  - *Tradeoff*: Summarizing outliers separately ensures no architectural information is lost, whereas ignoring them keeps compactions focused only on strong patterns.
  - *Proposal*: Group outliers into a single "Miscellaneous" chunk if there are any, so that no context is dropped.
- **Outlier Skills in Harvesting**: If a skill does not cluster with any other skill (outlier), should we skip conflict analysis for it?
  - *Tradeoff*: Outliers have no overlap with other skills, so comparing them would waste LLM prompt tokens. Skipping is the most efficient and logical option.
  - *Proposal*: Only run cross-skill conflict analysis on clusters of size >= 2. Outlier skills are not cross-analyzed.

## Blocking Questions
- None. These proposals are highly standard and robust. We will document them as requirements.
