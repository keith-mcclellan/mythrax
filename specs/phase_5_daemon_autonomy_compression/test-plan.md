# Test Plan: Phase 5 (v0.6.0) — Daemon Autonomy & Node Compression

This test plan defines the unit, integration, and acceptance tests to verify the correctness of Continuous Pruning, Inner-Node Compaction, the new Daemon CLI Run command, and Episode Filtering.

---

## Unit Tests

### U1: Stale Memory and Handoff Pruning
- **Location**: `mythrax-core/tests/test_stm.rs` or `mythrax-core/src/db/backend.rs`
- **Steps**:
  1. Insert a handoff record with `status: 'COMPLETED'` and `created_at` set to 4 days ago.
  2. Create a dummy file in the vault representing the handoff's markdown file, and dummy parent/subagent `stm_*.json` files.
  3. Insert an STM record into `short_term_memory` table with `updated_at` set to 4 days ago.
  4. Run `backend.prune_stale_memories(vault_path)`.
  5. Assert that:
     - The handoff record is deleted from SurrealDB.
     - The handoff markdown file is deleted from disk.
     - The parent/subagent `stm_*.json` files are deleted from disk.
     - The 4-day-old STM record is deleted from SurrealDB.

### U2: Fresh Memory Protection during Pruning
- **Location**: `mythrax-core/tests/test_stm.rs`
- **Steps**:
  1. Insert a handoff record with `status: 'COMPLETED'` and `created_at` set to 2 hours ago.
  2. Create dummy handoff and STM files on disk.
  3. Insert an STM record with `updated_at` set to 2 hours ago.
  4. Run `backend.prune_stale_memories(vault_path)`.
  5. Assert that:
     - The handoff record remains in SurrealDB.
     - All files remain on disk.
     - The fresh STM record remains in SurrealDB.

### U3: Inner-Node Compaction - Wisdom Rule
- **Location**: `mythrax-core/tests/test_compactor.rs`
- **Steps**:
  1. Insert a wisdom rule with a long causal explanation (`**Why**:`).
  2. Execute a search with a strict `token_budget` that is smaller than the full rule but large enough to fit the rule without the `Why` field.
  3. Assert that the search result is returned, its content does not contain `**Why**:`, and it fits within the token budget.

### U4: Inner-Node Compaction - Paragraph & Truncation
- **Location**: `mythrax-core/tests/test_compactor.rs`
- **Steps**:
  1. Insert a multi-paragraph episode note.
  2. Execute a search with a strict `token_budget` that fits only the first paragraph.
  3. Assert that the returned search result contains only the first paragraph with the suffix `... [Truncated (Inner-Node Compaction)]`.
  4. Execute another search with a very small budget that truncates the content in the middle of a paragraph. Assert that character-level truncation was executed to fit the budget exactly.

### U5: Default Search Excludes Episodes
- **Location**: `mythrax-core/src/db/backend.rs` (in unit tests)
- **Steps**:
  1. Insert one episode and one wiki node with identical tags.
  2. Run `backend.search(query, ..., include_episodes: false)`.
  3. Assert that the search results contain the wiki node, but NOT the episode.
  4. Run `backend.search(query, ..., include_episodes: true)`.
  5. Assert that both the wiki node and the episode are returned.

### U6: Graph Traversal Excludes Episodes by Default
- **Location**: `mythrax-core/src/db/backend.rs`
- **Steps**:
  1. Relate a wiki node to an episode and to another wiki node.
  2. Run `backend.search(query, ..., deep_insight: true, include_episodes: false)`.
  3. Assert that the related nodes return the related wiki node, but NOT the related episode.
  4. Run `backend.search(query, ..., deep_insight: true, include_episodes: true)`.
  5. Assert that both the related wiki node and related episode are returned.

---

## Integration Tests

### I1: Compactor and Dreaming Loop Integration
- **Location**: `mythrax-core/tests/test_compactor.rs`
- **Steps**:
  1. Set up a vault and run `Compactor::compact_scope` or `DreamCoordinator::run_dream`.
  2. Assert that `prune_stale_memories` is triggered and executes successfully without error.

---

## Acceptance & CLI Tests

### A1: Daemon CLI Run Command
- **Location**: `mythrax-core/tests/test_cli_e2e.rs`
- **Steps**:
  1. Run `cargo run -- daemon run --port 8091` as a background task.
  2. Perform an HTTP GET request to `http://127.0.0.1:8091/health` or status check.
  3. Verify that the server responds with success.
  4. Stop the daemon and check that the PID file is deleted.

### A2: CLI Search Episodes Flag
- **Location**: `mythrax-core/tests/test_cli_e2e.rs`
- **Steps**:
  1. Insert an episode.
  2. Run `cargo run -- search <query>`. Assert that no episodes are output.
  3. Run `cargo run -- search <query> --episodes`. Assert that the episode is returned in the output JSON.
