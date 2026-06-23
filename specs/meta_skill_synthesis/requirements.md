# Requirements: Meta-Skill Synthesis

## Problem
AI agents need to easily consume the wisdom rules and forged document knowledge stored in the Mythrax vault. Raw database rules and scattered markdown files are hard for the agent to retrieve cohesively during execution. We need a way to group, synthesize, and publish this knowledge back to the agent as standard, structured playbooks (Meta-Skills) that the agent automatically loads.

## Outcome
A CLI command and background pipeline that clusters active wisdom rules and forged wiki nodes, synthesizes high-level agent playbooks (`SKILL.md`), and writes them to the workspace's customizations root (`.agents/skills/`).

## User Value
- Automates the compilation of lessons learned into executable playbooks.
- Allows subsequent agents to immediately benefit from past experiences and designs in this repository.
- Avoids manual playbook writing.

## In Scope
- **Semantic Clustering**: Retrieve all `WisdomRule`s and `WikiNode`s from SurrealDB, get their embeddings, and group them using the DBSCAN algorithm.
- **LLM Synthesis**: For each cluster, format a targeted prompt containing the rules and wiki nodes in that cluster, and call the LLM to write a cohesive `SKILL.md` (including frontmatter and instructions).
- **File Publishing**: Write the generated playbooks to `.agents/skills/<name>/SKILL.md`.
- **Token Consciousness**: Ensure prompts do not exceed the context window by chunking/clustering and filtering inputs.
- **CLI & MCP Tool Integration**: Expose this capability via a new MCP tool `synthesize_meta_skills`.

## Out of Scope
- Modifying global skills (`~/.gemini/config/skills`). Only project-specific `.agents/skills/` will be written.
- Auto-deleting manually written playbooks.

## Inputs
- SurrealDB memory rules and wiki nodes.
- Existings playbooks in `.agents/skills` (for reference or enrichment).

## Outputs
- Synthesized directories `.agents/skills/meta_<cluster_name>/` containing `SKILL.md`.

## Acceptance Criteria
1. **Targeted Synthesizer**: Running the synthesis pipeline clusters records and generates at least one target playbook if matching rules/nodes exist.
2. **File Output**: Synthesized playbooks must have valid YAML frontmatter (containing `name` and `description`) and a markdown body.
3. **MCP Tool Integration**: The MCP server exposes `synthesize_meta_skills` and completes successfully.
4. **Token Consciousness**: High volume inputs are clustered and processed in separate API calls rather than a single giant prompt.
