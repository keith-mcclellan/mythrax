# Technical Design: Phase 5 (v0.6.0) — Daemon Autonomy & Node Compression

## 1. Overview & Architecture

This phase enhances the Mythrax Cognitive Daemon with self-cleaning capabilities (Continuous Pruning), context budget optimization (Inner-Node Compaction), improved operational control (Foreground Run mode), and context bloat prevention (Episode Filtering).

```mermaid
flowchart TD
    subgraph Daemon CLI (mythrax daemon run)
        A[Parse cli subcommand] --> B[Initialize SurrealBackend & MarkdownStore]
        B --> C[Spawn background file-watcher, dreaming, compaction, and periodic pruning loops]
        C --> D[Start Axum HTTP REST Server in Foreground]
        D --> E[Wait for SIGINT/Ctrl+C, then delete PID file and exit]
    end

    subgraph Periodic Pruning (Continuous Daemon Pruning)
        F[Triggered on startup/dreaming/compaction] --> G[DELETE FROM short_term_memory WHERE updated_at < time::now() - 3d]
        G --> H[Run delete_stale_handoffs to evict stale handoffs]
        H --> I[Scan .handoffs/ folder and delete stm_*.json files older than 3 days]
    end

    subgraph Inner-Node Compaction (SurrealBackend::search)
        J[Load Search Candidates] --> K{Fits in remaining budget?}
        K -- Yes --> L[Keep node intact]
        K -- No --> M[Call compact_search_result]
        M --> N[Is Wisdom Rule?]
        N -- Yes --> O[Strip **Why**: causal explanation field]
        N -- No --> P[Extract first paragraph or binary-search character truncation]
        O & P --> Q{Compacted node fits remaining budget?}
        Q -- Yes --> R[Keep compacted node, append suffix only if content changed]
        Q -- No --> S[Omit node and add to omitted_ids]
    end

    subgraph Episode Filtering (SurrealBackend::search)
        T[Receive include_episodes parameter] --> U{include_episodes == true?}
        U -- Yes --> V[Query episode, wiki_node, and wisdom tables. Include episode in related_targets.]
        U -- No --> W[Query wiki_node and wisdom tables only. Exclude episode from related_targets.]
    end
```

---

## 2. Detailed Components

### 2.1 Continuous STM & Handoff Pruning
- **Trait Definition** (`mythrax-core/src/db/backend.rs`):
  Add a method to the `StorageBackend` trait:
  ```rust
  async fn prune_stale_memories(&self, vault_root: &std::path::Path) -> Result<()>;
  ```
- **Implementation** (`SurrealBackend`):
  - Run SurrealDB query to delete old STM records (3-day threshold):
    ```sql
    DELETE FROM short_term_memory WHERE updated_at < time::now() - 3d;
    ```
  - Call the existing `delete_stale_handoffs()` function to prune stale handoffs and their linked parent/subagent STM files/records (which is updated to check `created_at < time::now() - 3d`).
  - Scan the `vault_root.join(".handoffs")` directory for any `stm_*.json` files. Read their last-modified metadata, and delete any file modified more than 3 days ago.
- **Integration Points**:
  - **Startup**: Call `prune_stale_memories` inside `Commands::Daemon` handler in `main.rs` before starting axum.
  - **Dreaming**: Call `backend.prune_stale_memories` at the end of `DreamCoordinator::run_dream` in `synthesis.rs`.
  - **Compacting**: Call `backend.prune_stale_memories` at the start of `Compactor::compact_scope` and `Compactor::compact_global` in `compactor.rs`.

### 2.2 Fine-Grained Inner-Node Compaction
- **Compaction Helper** (`db/backend.rs`):
  Add a helper function to `SurrealBackend`:
  ```rust
  fn compact_search_result(&self, item: &mut SearchResult, remaining_budget: usize) -> bool;
  ```
  - **Title Budget Check**: Check if the token count of `format!("{}\n", item.title)` exceeds `remaining_budget`. If so, return `false` (cannot fit even the title).
  - **Content Budget**: Let `content_budget = remaining_budget - title_tokens`.
  - **Wisdom Rule Compaction**:
    - If `item.content.contains("**Why**:")`, locate the string slice for `\n**Why**:` up to the next prefix `\n**Prescribed Remedy**:`.
    - Strip the causal explanation and build:
      ```rust
      let compacted = format!("{}\n{}", avoid_part, remedy_part);
      ```
    - Check if `self.count_text_tokens(&compacted) <= content_budget`. If so, update `item.content = compacted` and return `true`.
  - **Paragraph Compaction**:
    - Split the content by `\n\n`. If multiple paragraphs exist:
      - Construct `compacted = format!("{}\n\n... [Truncated (Inner-Node Compaction)]", paragraphs[0])`.
      - If `self.count_text_tokens(&compacted) <= content_budget`, set `item.content = compacted` and return `true`.
  - **Binary Search Character Truncation**:
    - Perform a binary search on the character length of `item.content` to find the maximum substring length `mid` that fits.
    - Candidate text: `format!("{}... [Truncated (Inner-Node Compaction)]", &original_content[..mid])`.
    - If the maximum fitting substring has length `mid > 0`, update `item.content` to this candidate text and return `true`.
    - Otherwise, return `false`.
  - **Suffix Condition**: The suffix `... [Truncated (Inner-Node Compaction)]` must *only* be appended if the original text was actually shortened.

- **Search Retrieval Integration** (`db/backend.rs` at `search` method):
  - Replace the current hard truncation logic with the new compaction logic. If a candidate exceeds the budget, attempt to compact it. If it successfully compacts, add its new token count to `cumulative_tokens` and push the compacted item to `kept`.

### 2.3 Episode Filtering & Exclusion
- **Trait Definition** (`mythrax-core/src/db/backend.rs`):
  Update `search` signature:
  ```rust
  async fn search(
      &self,
      query: &str,
      scope: Option<&str>,
      deep_insight: bool,
      limit: usize,
      offset: usize,
      threshold: f32,
      token_budget: Option<usize>,
      allow_downward: bool,
      include_episodes: bool, // New parameter
  ) -> Result<SearchResponse>;
  ```
- **SurrealBackend Implementation**:
  - Traversal Target Filtering:
    - If `include_episodes` is `true`:
      Traversal target list is: `episode, entity, wiki_node, wisdom, hypothesis_node, handoff`.
      The traversal arrow string is `"<->"` if `allow_downward` is true, otherwise `->`.
    - If `include_episodes` is `false`:
      Traversal target list is: `entity, wiki_node, wisdom, hypothesis_node, handoff` (explicitly excluding `episode`).
      The traversal arrow string is `"<->"` if `allow_downward` is true, otherwise `->`.
  - SQL Search Query Setup:
    - If `include_episodes` is `true`, execute SurrealDB statement querying: `episode`, `wiki_node`, `wisdom` (3 queries in parallel).
    - If `include_episodes` is `false`, execute SurrealDB statement querying: `wiki_node`, `wisdom` (2 queries in parallel).
    - Map result arrays appropriately (since query array index shifts from 3 to 2).
- **Interfaces (REST API, MCP, CLI)**:
  - Add `include_episodes` boolean parameter to Axum `/v1/search` and MCP `search_memories`.
  - Add `episodes: bool` flag to CLI `search` command.

### 2.4 Daemon CLI `run` Subcommand
- **CLI updates** (`cli.rs`):
  Modify `DaemonAction` to include `Run { port: u16, vault: Option<String> }`.
- **CLI Handling & Signal Exit** (`main.rs`):
  - Both `Start` and `Run` launch Axum and write a PID file to `~/.mythrax/daemon.pid`.
  - For `DaemonAction::Run`, wrap the Axum server execution in a `tokio::select!` block listening for Ctrl+C / SIGINT:
    ```rust
    tokio::select! {
        res = axum::serve(listener, app) => {
            if let Err(e) = res {
                tracing::error!("Daemon server crashed: {:?}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("SIGINT/Ctrl+C received. Cleaning up PID file and shutting down...");
        }
    }
    let _ = std::fs::remove_file(pid_path);
    ```

---

## 3. Data Flow & Interfaces
- **StorageBackend Trait**: Added `prune_stale_memories` async function. Update `search` function parameters.
- **SearchResult Struct**: Fields remain unchanged. Content is mutated in place by the compaction logic before returning the search response.

---

## 4. Safety Boundaries & Error Handling
- **Compaction Fallback**: If a candidate node cannot be compacted to fit the remaining budget (e.g. remaining budget is too small), it is omitted entirely and added to `omitted_ids` as before, ensuring we never exceed the specified `token_budget` under any circumstance.
- **Pruning Safety**: File deletions use `std::fs::remove_file` wrapped in error ignores to ensure that minor filesystem permissions or missing files do not crash the background daemon or compactor run.
- **Database Safety**: RocksDB embedding generation uses single-threaded locking semaphores to prevent database lock contention.
