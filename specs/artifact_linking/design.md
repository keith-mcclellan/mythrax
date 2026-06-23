# Design: Artifact Ingestion Linking & Local Model Stabilization

## Overview
We will implement:
1. **Ingestion Chunking**: Split parsed conversation logs into parts (max 100,000 characters) during ingestion. This prevents context loss from simple truncation, keeps chunks within safe memory boundaries, and maintains local LLM stability.
2. **Bidirectional wikilinks**: Link all episode parts to their generated artifacts in Obsidian markdown files.
3. **Directed database edges**: Connect each episode chunk to the conversation's artifacts in SurrealDB.
4. **Database retrieval**: Add a trait method to query related node IDs for a given episode.
5. **Enriched LLM Dreaming Prompt**: Modify the LLM prompt construction inside the dreaming/synthesis loop to retrieve and append related artifacts, capping the final combined prompt size at 100,000 characters.
6. **Local LLM Client Stabilization**: Resize output token sizes dynamically and implement robust exponential retry backoffs.

## Execution Flow

1. **Pre-scan Artifacts**: 
   Before processing the raw episode log during `antigravity` ingestion, scan the conversation directory (`path`) for all `.md` files (artifacts).
   Collect their file stems (e.g. `walkthrough`, `implementation_plan`) and contents.

2. **Chunk Parsed Content**:
   Parse the conversation log (`transcript.jsonl`) into markdown.
   Use a chunking utility to split the parsed log into chunks of at most 100,000 characters, respecting line boundaries where possible.

3. **Write Episode Parts & Save to DB**:
   For each chunk:
   - Generate part-specific titles: `antigravity_{dir_name}_part{N}` (or `antigravity_{dir_name}` if only 1 part).
   - Append a `## Linked Artifacts` section at the bottom containing Obsidian wikilinks to all pre-scanned artifacts:
     `- [[wiki/artifacts/{conv_id}/{file_stem}]]`
   - Write the chunk to `episodes/antigravity_{dir_name}_part{N}_{uuid_short}.md`.
   - Save the chunk using `db.save_episode` to obtain `episode_saved_id`. Keep track of all `(part_title, relative_path, episode_saved_id)` tuples.

4. **Modify & Write Artifacts**:
   For each pre-scanned artifact:
   - Append a backlink footer referencing all generated episode parts:
     `\n\n---\nSource Episodes: [[episodes/antigravity_{dir_name}_part1_{uuid_short}|antigravity_{dir_name}_part1]] | ...\n`
   - Write the modified content to the Obsidian vault at `wiki/artifacts/{conv_id}/{file_stem}.md`.
   - Save the artifact to SurrealDB using `db.save_wiki_node` to obtain `wiki_node_id`.
   - Connect each episode part to the artifact by calling `db.relate_nodes(&part_episode_saved_id, &wiki_node_id).await`.

5. **Database Query for Related Node IDs**:
   In `db/backend.rs`, add a trait method:
   `async fn get_related_node_ids(&self, from_id: &str) -> Result<Vec<String>>`
   Implementation in `SurrealBackend`:
   `SELECT VALUE out FROM relates_to WHERE in = $from;`

6. **Enriched LLM Dreaming Prompt**:
   In `synthesis.rs`, for each episode part being summarized (in incremental merge or cluster analysis):
   - Query `db.get_related_node_ids(ep_id)`.
   - Query `db.get_memory_nodes(related_ids)`.
   - Iterate through the returned `WikiNodes` (artifacts) and format their name/content:
     `Artifact Name: <name>\nContent:\n<content>\n`
   - Append this to the episode's content variable passed into the LLM prompt under an `Associated Artifacts:` section.
   - Apply a safety context window truncation capping the final combined prompt size to `100,000` characters before prompting.

7. **Local LLM Client Stabilization**:
   In `llm/mod.rs`:
   - Set `"max_tokens": 8192` by default in local completion payloads.
   - Truncate any incoming prompt exceeding 100,000 characters to prevent OOM/GPU driver timeouts.
   - Implement a 5-second pause (`tokio::time::sleep`) after each local completion request (while holding the semaphore) to give the local model/GPU time to clear its cache and cool down.
   - Modify `send_with_retry` to execute up to 6 attempts (5 retries) with a base delay of `500.0` milliseconds and a capped maximum wait time of `5.0` seconds per retry.
   - Print a warning log if connection is refused:
     `"WARNING: Local LLM connection refused. If the server crashed, run: brew services restart mlx-lm"`

## Tradeoffs
- Splitting episodes creates more files and nodes, but it significantly improves vector search accuracy (since embedding vectors represent focused 8KB chunks rather than massive 740KB texts), prevents GPU memory crashes during local LLM execution, and speeds up DBSCAN clustering.

