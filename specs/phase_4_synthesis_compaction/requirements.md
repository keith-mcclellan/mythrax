# Requirements: Phase 4 (v0.5.0) — Advanced Synthesis & Compaction

## Problem
Currently, the advanced synthesis and compaction stages of the Mythrax Cognitive Pipeline are unoptimized:
1. Scope compactions slice insights sequentially into fixed chunks of 5 rather than grouping them semantically, which leads to disjointed summaries.
2. New episodes are incrementally merged into existing insights during dreaming without monitoring drift, which can cause insights to grow bloated and lose cohesion over time.
3. New wisdom rules are appended blindly without deduplication or generalization, leading to duplicate rules.
4. Cross-skill conflict analysis is run monolithically on all global and project skills, wasting tokens and diluting conflict detection.

## Outcome
1. Scope insights are grouped semantically using DBSCAN before compactions are generated. Outlier insights are compacted in a fallback chunk.
2. Pairwise distance is monitored on all source episodes for each insight during dreaming. If the maximum distance exceeds `0.30`, the insight is subdivided into smaller insights.
3. Newly generated wisdom rules are compared to existing rules using embedding similarity. Rules with similarity `> 0.80` are merged and generalized via the LLM, while keeping `tier: "skills"` rules as read-only anchors.
4. Skills are clustered by description similarity using DBSCAN. Cross-skill conflict analysis is only executed on clusters of size >= 2.

## User Value
- High cohesion and zero bloat in Obsidian notes.
- Less redundant rules in vector search.
- Faster, more token-efficient dreaming and skill harvesting runs.

## In Scope
1. Refactoring `Compactor::compact_scope` to cluster insights using DBSCAN.
2. Implementing drift monitoring and subdivision splitting in `DreamCoordinator`.
3. Implementing wisdom rule deduplication and generalization during dreaming and document forging.
4. Refactoring `Harvester::harvest_skills` to cluster skills before running conflict analysis.

## Out of Scope
- Implementing network-based SurrealDB connection (stays embedded RocksDB).
- Fine-grained inner-node token budgeting compaction (remains node truncation).

## Inputs
- Scope name, insight notes, episode notes, skill notes, and their embeddings.

## Outputs
- Structured Markdown compaction files (`wiki/compaction/*`).
- Drifting insight splits (files deleted and new ones created).
- Generalized wisdom rules in `wisdom/dynamic/`.
- Cross-skill conflict wisdom rules in `wisdom/skills/`.

## Constraints
- Max pairwise distance threshold is `0.30` for splits.
- Similarity threshold is `0.80` for wisdom rule merging.
- Standard DBSCAN parameters: `eps = 0.10`, `min_samples = 2` for insights and skills.

## Assumptions
- Custom DBSCAN function `dbscan` in `synthesis.rs` is fully reusable.
- Embeddings are available for all insights, episodes, and skill descriptions.

## Risks and Edge Cases
- **No clusters formed**: If all insights or skills are outliers, DBSCAN returns all labels as `None`. We must handle this gracefully (fallback to a single miscellaneous chunk for compactions, or skip conflict analysis for skills).
- **SurrealDB relationship cleanups**: When an insight is split, the relations between the old insight and its source episodes must be deleted, and new relations built.

## Acceptance Criteria
- [ ] `Compactor::compact_scope` clusters insights using DBSCAN, grouping outliers into a miscellaneous compaction chunk.
- [ ] `DreamCoordinator` monitors pairwise distance of episodes in each insight. If max distance > 0.30, it runs DBSCAN on the episodes and splits the insight.
- [ ] Rules generated during dreaming or forging are compared against existing rules; if similarity > 0.80, they are generalized (or skipped if matched with a skills rule).
- [ ] `Harvester::harvest_skills` clusters skills by description using DBSCAN, executing cross-skill analysis only on clusters of size >= 2.
- [ ] 100% test pass rate on all new and existing tests.
