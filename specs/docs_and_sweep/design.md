# Design: Documentation Update & Background Transcript Sweep

## Proposed Architecture & Execution Flow
The background transcript sweep runs inside `DreamCoordinator::run_dream` in [synthesis.rs](file:///Users/keith/Documents/mythrax/mythrax-core/src/cognitive/synthesis.rs). It integrates before the compactor retrieves unprocessed episodes to compile digests.

### Detailed Execution Sequence
1. **Compactor Boot**: `run_dream` is called.
2. **Query Registered Session Transcripts**:
   The compactor queries the `short_term_memory` table for all sessions with stashed transcript paths:
   ```sql
   SELECT session_id, value AS path, updated_at FROM short_term_memory WHERE key = '_transcript_path';
   ```
3. **Inactivity Evaluation**:
   For each session row:
   - Compute `idle_duration = time::now() - updated_at`.
   - If `idle_duration > 10 minutes`:
     - Validate that the transcript file exists at `path`. If missing, log a warning and delete the STM key to prevent retry loops.
     - Call `mine_transcript(session_id, path)` to parse and insert trailing turns.
     - Upon success, execute a delete query to remove the `_transcript_path` key from that session's STM table:
       ```sql
       DELETE FROM short_term_memory WHERE session_id = $session_id AND key = '_transcript_path';
       ```
4. **Digest Compaction**:
   Call `db.get_unprocessed_episodes()` (which now includes the freshly mined trailing turns) and compile summaries as normal.

---

## Interfaces, Inputs, and Outputs
We reuse the existing `mine_transcript` function from the `hooks::precompact` module:
```rust
pub async fn mine_transcript(
    session: &str,
    transcript_path: &str,
    backend: &Arc<dyn StorageBackend>,
    store: &Arc<MarkdownStore>,
    ignore: &WatchIgnoreList,
) -> Result<usize>
```

---

## State Management & Database Constraints
* **STM Table**: The short-term memory table acts as the registry, storing session IDs, their transcript paths, and update times.
* **Idempotence**: Mined episodes are saved via `save_episode_bidirectional` which computes content hashes to prevent duplicate markdown/database entries if the same file is parsed multiple times.

---

## Error Handling
* If `mine_transcript` fails, we catch the error, log a warning, and do *not* delete the STM key. This allows the sweep to retry on the next dreaming cycle.
* If a transcript path is invalid or pointing to a non-existent file, we delete the key to avoid infinite errors.

---

## Safety Boundaries
* **Gated Execution**: Only run during dreaming (idle compactions), ensuring file reads and token parsing occur when user/agent traffic is zero, preserving Metal GPU resources for interactive commands.
