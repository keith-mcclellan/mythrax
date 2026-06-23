# Clarify: Artifact Linking & Local Model Stabilization

## Restated Request
Currently, during `mythrax init antigravity` or incremental ingestion, markdown artifacts (like walkthroughs, implementation plans, and tasks) are ingested as `WikiNodes`, and conversation logs are ingested as `episodes`. However:
1. There are no links defined between the raw episode note and its corresponding artifacts in the Obsidian vault (e.g. `[[wiki/artifacts/...]]` or `[[episodes/...]]`).
2. There are no relationships (edges) defined in the SurrealDB graph database between the raw episode node and the artifact wiki nodes.
3. The dreaming/summarization logic (`DreamCoordinator`) does not pull in the linked artifacts when summarizing episodes to generate or refine insights.
4. During bulk operations (like `vault summarize`), the local LLM server (`mlx-lm` running `Qwen3.6-35B-A3B-4bit`) crashes or times out due to GPU memory pressure and short HTTP retry backoffs, causing connection refused errors.

The user wants us to:
1. Establish bidirectional links/relationships between raw episodes and their respective conversation artifacts.
2. Ensure that during dreaming (insight generation/refinement), any associated artifacts linked to the episodes are pulled into the LLM prompt to inform the summary.
3. Stabilize local LLM execution during bulk operations by preventing memory crashes and improving network retry/recovery resilience.
4. Clarify the token configuration (input vs output), how to handle larger objects, and whether client timeouts need adjustment.

## Known Facts
- Raw episodes are written to `episodes/antigravity_{dir_name}_{uuid_short}.md`.
- Artifacts are written to `wiki/artifacts/{conv_id}/{file_stem}.md`.
- `conv_id` and `dir_name` are identical (both are the conversation ID UUID).
- Directed relationships `episode -> relates_to -> wiki_node` can be established using `db.relate_nodes(from_id, to_id)`.
- `DreamCoordinator` uses `llm.completion` to synthesize/summarize episodes into insights in `mythrax-core/src/cognitive/synthesis.rs`.
- Local LLM runs on `http://127.0.0.1:8080/v1/chat/completions`.
- The error `connection closed before message completed` followed by `Connection refused` indicates that the local `mlx-lm` server crashed mid-execution (often due to Out-Of-Memory or macOS GPU driver timeout during heavy prompt processing), not that the client timed out (the client has no timeout).

## Token and Timeout Analysis
- **max_tokens is for Output (generation) tokens**: In the local OpenAI API payload, `max_tokens: 16384` instructs the server to allow/allocate memory for generating up to 16k tokens. For a 35B model, pre-allocating a KV cache of this size for small summaries creates massive unnecessary memory pressure, causing the local server to crash. We should reduce this to `2048` for summaries/compaction and `4096` for code changes.
- **Handling Larger Objects**: To handle large contexts without crashing the local GPU, we cap the combined input prompt at `12000` characters (~3,000 tokens). If the user needs to process larger objects (e.g. massive transcripts or multiple full artifacts) without truncation, they should switch to the `cloud` provider (e.g., Gemini) which has 1M+ token context windows.
- **Client Timeout**: Since the client does not set a timeout, it waits indefinitely. The connection failure is due to a server-side crash, so adjusting client timeouts will not help. Instead, robust backoff retries with longer waits (up to 5s per retry, total ~17s) are needed to let the server recover or auto-restart if it crashes.

## Chunking Analysis
- **Context Loss Concern**: A single conversation can grow very large (e.g. 746K characters or ~200k tokens). Simply truncating the episode at 8k characters leaves less than 1% of the conversation context.
- **ONNX Model Limits**: Standard embedding models (like Nomis ONNX) have maximum input token limits (e.g., 2048 or 8192 tokens). Running embeddings on a 746K character string results in silent truncation where only the very beginning of the transcript is searchable.
- **The Solution (Ingestion Chunking)**: By chunking the parsed logs into parts (max 8,000 characters) during ingestion:
  1. We avoid context loss from simple truncation (each part is summarized/processed).
  2. Each part gets its own focused embedding, making vector search extremely precise.
  3. Prompts are kept small and fast, avoiding any chance of GPU memory pressure (Metal OOM) on the local server.
  4. Each part is linked bidirectionally to the conversation's artifacts.

