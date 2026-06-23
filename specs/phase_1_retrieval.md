# Specification: Core Retrieval & Skill Isolation (v0.2.0)

This specification defines the implementation details, requirements, design choices, and verification plan for Phase 1 of the Mythrax memory enhancements.

---

## 1. Clarify

### Restated Request
Implement Phase 1 (v0.2.0) of the Mythrax Cognitive Pipeline & Retrieval Roadmap, focusing on:
1. **Unified Multi-Tier Vector Search**: Searching `wisdom`, `wiki_node`, and `episode` tables in parallel.
2. **Segregation of Skill Wisdom**: Saving harvested skill playbooks to `tier: "skills"` under `vault/wisdom/skills/` and prioritizing them during retrieval.
3. **Direct Node Lookup**: Adding a `get_memory_nodes(node_ids)` MCP endpoint to hydrate specific database records.

### Known Facts
* Standard HNSW indices are already defined for the `episode`, `wisdom`, and `wiki_node` tables.
* A RocksDB-backed filesystem watcher (`RecommendedWatcher`) parses markdown frontmatter and syncs files under `episodes/`, `wisdom/`, and `wiki/` automatically.
* In `mythrax-core`, the `LLMClient` handles client completions, and the `SearchResult` contract maps standard outputs.

### Assumptions
* A **Tier Boost Multiplier** will be applied to the search similarity scores to prioritize higher-order knowledge:
  - `skills` / `wisdom` $\rightarrow$ multiplier = `1.2`
  - `wiki_node` / `insights` / `briefs` $\rightarrow$ multiplier = `1.1`
  - `episode` $\rightarrow$ multiplier = `1.0`
* The final blended score will follow the formula:
  $$\text{blended\_score} = \text{similarity} \times (0.7 + 0.3 \times \text{utility}) \times \text{tier\_boost}$$
* Wisdom Rules projected to `SearchResult` will have their `content` field mapped to a clean Markdown block detailing the action to avoid, explanation, and remedy.

### Tradeoffs
* **SurrealDB Multi-Table Querying**: Querying multiple tables via separate SELECT statements in a single batch is chosen over a comma-separated `FROM` statement. This permits custom field aliasing and formatting (e.g. mapping `name` to `title` for wiki nodes, or constructing the Markdown content block for wisdom rules).

### Blocking Questions
* None. All core design choices have been clarified via the `/grill-me` session.

---

## 2. Requirements

### Problem Statement
Currently, vector search is locked to the `episode` table. The agent has no semantic pathway to locate high-fidelity compactions (briefs) or wisdom rules unless a raw developer episode is matched. Moreover, user-defined skill playbooks are saved as "dynamic" rules, risking their corruption or dilution during future generalization/compaction activities.

### Outcome
Agents can query all memory tiers semantically in a single call. User-defined skill constraints are safely isolated and take precedence over empirical logs. Subagents can load precise node context by ID using STM pointers.

### User Value
Drastically reduced context window bloat, improved instruction accuracy, and absolute safety of user skills.

### In Scope
1. Refactoring `search_memories` to query `episode`, `wiki_node`, and `wisdom` in SurrealDB.
2. Implementing the Tier Boost ranking logic and Markdown projection formatting.
3. Updating the skill harvester to save playbooks under `tier: "skills"` in the `vault/wisdom/skills/`.
4. Adding `tier: "skills"` precedence support in memory queries.
5. Implementing the `get_memory_nodes` MCP tool and REST API endpoint.

### Out of Scope
* Up-tree and down-tree graph walking (v0.3.0).
* Selective traversal and token budgeting (v0.3.0).
* TOC-driven document chunking (v0.4.0).

### Inputs
* `search_memories` arguments: `query` (String), `scope` (Option<String>), `limit` (usize), `offset` (usize), `threshold` (f32).
* `get_memory_nodes` arguments: `node_ids` (Vec<String>).

### Outputs
* `SearchResponse`: Array of unified `SearchResult` items sorted by blended score.
* `GetMemoryNodesResponse`: Struct containing lists of fully hydrated `episodes`, `wisdom_rules`, and `wiki_nodes`.

### Acceptance Criteria
- [ ] Unified search queries `episode`, `wiki_node`, and `wisdom` tables.
- [ ] Wisdom rules in search results project their properties as a structured Markdown block inside the `content` field.
- [ ] Search results are sorted by the blended score formula, applying the correct tier multipliers.
- [ ] Harvested skill rules write `tier: "skills"` in their frontmatter and are saved in `vault/wisdom/skills/`.
- [ ] Memory searches rank `tier: "skills"` rules above other tiers when similarity is comparable.
- [ ] `get_memory_nodes` successfully parses node IDs, queries their tables, and returns grouped raw database objects.

---

## 3. Design

### Data Mapping to `SearchResult`
1. **`episode`**:
   - `id` $\rightarrow$ `episode:<uuid>`
   - `title` $\rightarrow$ `title`
   - `content` $\rightarrow$ `content`
   - `tier` $\rightarrow$ `"episode"`
2. **`wiki_node`**:
   - `id` $\rightarrow$ `wiki_node:<uuid>`
   - `title` $\rightarrow$ `name`
   - `content` $\rightarrow$ `content`
   - `tier` $\rightarrow$ Detect type (e.g. `"project_brief"`, `"system_playbook"`, or default to `"insight"`)
3. **`wisdom`**:
   - `id` $\rightarrow$ `wisdom:<uuid>`
   - `title` $\rightarrow$ `target_pattern`
   - `content` $\rightarrow$ Format:
     ```markdown
     **Action to Avoid**: <action_to_avoid>
     **Why**: <causal_explanation>
     **Prescribed Remedy**: <prescribed_remedy>
     ```
   - `tier` $\rightarrow$ the wisdom `tier` value (e.g. `"skills"`, `"dynamic"`, `"forge"`)

### Execution Flow: `search` Method
```text
1. Embed the query via nomic-embed-text.
2. In SurrealDB, run three parallel SELECT statements targeting episode, wiki_node, and wisdom.
3. Collect and merge the candidates.
4. For each candidate:
   a. Compute cosine similarity (dot product of embeddings).
   b. Retrieve utility score (default to 1.0 if missing).
   c. Determine tier boost factor (Skills/Wisdom = 1.2, Wiki = 1.1, Episode = 1.0).
   d. Calculate blended_score = similarity * (0.7 + 0.3 * utility) * tier_boost.
5. Filter candidates where blended_score >= threshold.
6. Sort candidates by blended_score DESC.
7. Slice the results by offset and limit.
8. Return SearchResponse.
```

### Execution Flow: `get_memory_nodes` Method
```text
1. Accept Vec<String> of node IDs.
2. Initialize GetMemoryNodesResponse (episodes, wisdom_rules, wiki_nodes).
3. Loop over each ID:
   a. Parse into RecordId (table name + key).
   b. Match table:
      - "episode": Query `episode` table, deserialize into Episode struct, push to episodes list.
      - "wisdom": Query `wisdom` table, deserialize into WisdomRule struct, push to wisdom_rules list.
      - "wiki_node": Query `wiki_node` table, deserialize into WikiNode struct, push to wiki_nodes list.
4. Return GetMemoryNodesResponse.
```

---

## 4. Test Plan

### Unit Tests
* **Test Ranking**: Create mock search candidates of different tiers with fixed similarities and utilities. Verify they sort correctly according to the blended score formula.
* **Test Formatting**: Verify that a wisdom rule maps its target pattern, action, explanation, and remedy into the markdown block format in the `SearchResult` content field.

### Integration Tests
* **Test Multi-Tier Search Query**: Seed mock data into SurrealDB (one episode, one wiki node, one wisdom rule). Call `search` and verify all three are returned.
* **Test Skills Harvesting**: Run the harvester. Verify that a new skill rule is written to the `vault/wisdom/skills/` directory with `tier: "skills"` in the frontmatter.
* **Test Watcher Integration**: Create a file in `vault/wisdom/skills/`. Verify the file watcher automatically parses it and inserts it into the `wisdom` SurrealDB table with `tier: "skills"`.
* **Test Node Hydration**: Seed a mock episode and a mock wisdom rule. Query `get_memory_nodes` with their IDs and verify they are correctly hydrated.

---

## 5. Implementation Tasks

### T1: Refactor Ingestion & File Watcher for the Skills Tier
- **Actions**:
  - Update `src/cognitive/harvest.rs` to write skill rules to `wisdom/skills/` and set `tier: "skills"` in frontmatter and the `WisdomRule` struct.
  - Update `src/vault/watcher.rs` to parse files in `wisdom/skills/` as `tier: "skills"` if missing from frontmatter.
- **Verification**: Run `cargo test` and verify skill file outputs.

### T2: Implement the `get_memory_nodes` Endpoint
- **Actions**:
  - Create the `GetMemoryNodesResponse` contract in `src/contracts.rs`.
  - Add the `get_memory_nodes` query logic in `src/db/backend.rs` (implementing `StorageBackend` trait).
  - Add the `get_memory_nodes` handler in `src/mcp.rs` and register it as an MCP tool in `tools/list`.
  - Add the `get-memory-nodes` route to the Axum REST API in `src/api.rs` and `src/main.rs`.
- **Verification**: Query `get_memory_nodes` via JSON-RPC or REST and verify correct output.

### T3: Refactor Search Logic for Multi-Tier Vector Search
- **Actions**:
  - Update the SQL query in `src/db/backend.rs:search` to fetch records from `episode`, `wiki_node`, and `wisdom` tables.
  - Implement field projection and Markdown formatting for `wisdom` rule candidates.
  - Implement the Tier Boost Multiplier and sorting logic in Rust.
- **Verification**: Run query tests and verify that compactions and rules rank higher than raw episodes.

---

## 6. Validation

### Acceptance Criteria Review
*To be filled out during the validation phase.*

### Test Results
*To be filled out during the validation phase.*

### Final Status
*PENDING*
