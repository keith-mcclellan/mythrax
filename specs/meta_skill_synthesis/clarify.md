# Clarify: Meta-Skill Synthesis

## Restated Request
Implement a new cognitive capability ("meta-skill synthesis") that reads wisdom rules, forged documents, and existing skills, and synthesizes them into structured, high-level agent playbooks (Meta-Skills) published to the agent's customizations root (`.agents/skills/<skill-name>/SKILL.md`). The implementation must be token-conscious, integrated with the existing cognitive layers, and avoid duplicating existing features.

## Known Facts
1. **Existing Skills**: Stored as directories containing `SKILL.md` with YAML frontmatter (name, description) and a markdown body. Located under global config (`~/.gemini/config/skills`) or project customizations (`.agents/skills`).
2. **Existing Ingestion/Harvesting**:
   - `harvest_skills`: Reads existing playbooks, clusters them using DBSCAN, and extracts *Wisdom Rules* to resolve conflicts or combine playbooks.
   - `ingest_document`: Parses raw docs, extracts *Wisdom Rules* and *Wiki Nodes*.
3. **Current DB Structs**:
   - `WisdomRule` represents dynamic patterns/actions to avoid.
   - `WikiNode` represents forged document contents (wiki/artifacts, etc.).
4. **Token Limits**: High-volume inputs (many rules + docs) can exceed model context limits. A token-conscious approach is mandatory.

## Assumptions
1. **Target Directory**: Synthesized meta-skills will be written to `.agents/skills/<meta-skill-name>/SKILL.md` (project customization root) so they are automatically discovered and loaded by future agent instances in this workspace.
2. **Triggers**: Synthesized playbooks will include trigger rules in their description or body so that they are activated by standard matching.
3. **No Duplicate Execution**: We should extend the existing `Harvester` or create a companion `Synthesizer` that manages this pipeline.

## Tradeoffs & Token-Conscious Strategies
1. **Option A: Global Dump**: Retrieve all wisdom rules, forged docs, and existing skills, and pass them to the LLM.
   - *Pros*: Simple to write.
   - *Cons*: Extremely expensive, easily hits token limits (NOT token conscious).
2. **Option B: Cluster-Based Synthesis (Recommended)**:
   - Run DBSCAN clustering on the embeddings of active `WisdomRule`s and `WikiNode`s.
   - For each cluster (e.g. `cluster_0` = Git workflows, `cluster_1` = SurrealDB optimization):
     - Retrieve only the rules and wiki nodes belonging to that cluster.
     - Look up any existing meta-skill matching the cluster name or description.
     - Call the LLM to synthesize/update the specific `SKILL.md` for this cluster.
   - *Pros*: Highly token conscious, creates modular and targeted playbooks instead of one giant rulebook.

## Blocking Questions
None. The DBSCAN clustering approach directly leverages the existing vector/synthesis architecture of Mythrax.
