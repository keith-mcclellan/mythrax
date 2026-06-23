# Test Plan: Artifact Ingestion Linking & Local Model Stabilization

## Integration & Unit Tests
We will add/expand tests in `tests/test_vault_lifecycle.rs`, `synthesis.rs`, and `llm/mod.rs` tests.
Specifically, in `mythrax-core/tests/test_vault_lifecycle.rs`:
- Create a mock Antigravity transcript structure with `transcript.jsonl`, `walkthrough.md`, and `implementation_plan.md`.
- Run `bulk_ingest_vault` for the mock folder.
- Assertions:
  1. The raw episode file under `episodes/` contains a `## Linked Artifacts` section.
  2. The walkthrough artifact file in `wiki/artifacts/...` contains the `Source Episode` backlink footer.
  3. Query SurrealDB to check if the `relates_to` relationship exists.
- In `synthesis.rs` or `test_compactor.rs`:
  - Run the `DreamCoordinator::run_dream` function on a mock database containing an episode linked to a wiki node.
  - Verify that the prompt sent to the mock LLM contains the text of the linked wiki node (the artifact), and is bounded by the safety context length.
- In `llm/mod.rs`:
  - Verify the dynamic setting of `max_tokens` payload (e.g. 2048 vs 4096 based on prompt contents).
  - Verify that `send_with_retry` successfully retries on HTTP errors.
