# Verification Review Log

This document records the critical test execution outputs, observed behaviors, divergences, and documentation adjustments for each mapped system flow in Mythrax 2.0.

---

## 1. Loop Verification State Tracker

| Item | Source | Priority | Status | Confirmed_by | Owner | Last Action | Next Action | Evidence | Baseline | Human Review | Updated_at |
|---|---|---|---|---|---|---|---|---|---|---|---|
| **T1: Background Sweep** | Code | High | `done` | `confirmed:tests` | local_code_writer | implemented in synthesis.rs | none | cargo test ok | compile error fixed | yes | 2026-06-27 |
| **T2: CLI Timeout** | Code | Medium | `done` | `confirmed:tests` | local_code_writer | adjusted main.rs from 5s to 15s | none | cargo test ok | compile error fixed | yes | 2026-06-27 |
| **T3: Sweep Test** | Code | High | `done` | `confirmed:tests` | local_code_writer | created test_abandoned_session_sweep.rs | none | test passed | compile error fixed | yes | 2026-06-27 |
| **T3b: Parallelize** | Code | High | `done` | `confirmed:tests` | local_code_writer | installed nextest, fixed DB locks | none | nextest ok (67s) | lock contention resolved | yes | 2026-06-27 |
| **T5: Flow 1 Verify** | Docs | High | `done` | `confirmed:tests` | orchestrator | verified Flow 1 startup/WAL | none | nextest ok | none | yes | 2026-06-27 |
| **T6: Flow 2 Verify** | Docs | High | `done` | `confirmed:tests` | orchestrator | verified Flow 2 precompact ingestion | none | nextest ok | none | yes | 2026-06-27 |
| **T7: Flow 3 Verify** | Docs | Medium | `done` | `confirmed:tests` | orchestrator | verified Flow 3 request routing | none | nextest ok | none | yes | 2026-06-27 |
| **T8: Flow 4 Verify** | Docs | High | `done` | `confirmed:tests` | orchestrator | verified Flow 4 memory retrieval | none | nextest ok | none | yes | 2026-06-27 |
| **T9: Verify Sweep** | Docs | High | `done` | `confirmed:tests` | orchestrator | verified Flow 5 compactor sweep | none | nextest ok | none | yes | 2026-06-27 |

---

## 2. Sequential Verification Logs

### Flow 1: Startup, Bootstrapping & Self-Healing (T5)
*   **Verification Command**: `cargo nextest run --test test_non_blocking_daemon` and code review.
*   **Expected Behavior**:
    - The CLI detects if the daemon is active, spawns it if inactive, and polls readiness.
    - Database handles file locks (`LOCK` file contention) safely on startup via connection retries.
    - Self-healing replays database writes from WAL logs (`replay_wal_if_fresh`) if the database is uninitialized or empty.
*   **Observed Behavior**: 
    - `test_thread_safe_wal_concurrency_and_robust_replay_marker_compaction` passed successfully in 6.1s.
    - Verified that `replay_wal_if_fresh` parses transaction logs and applies them to the database, writing the `.initialized` marker to prevent duplicate replays.
    - Spawns background WAL receiver loop to log subsequent modifications.
*   **Divergences**: None. The implementation works exactly as specified.
*   **Status**: `verified`

### Flow 2: Pre-Compaction Hook & Verbatim Ingestion (T6)
*   **Verification Command**: `cargo nextest run --test test_precompact_ingest`
*   **Expected Behavior**:
    - Hook parses flat or Claude/Gemini block-array user/tool transcript turns.
    - Captures terminal outputs and errors verbatim, saving them to SurrealDB without truncation.
*   **Observed Behavior**:
    - `precompact_persists_raw_tool_output` passed successfully. Verifies parsing of flat JSON message format with verbatim extraction.
    - `precompact_persists_array_form_tool_result_blocks` passed successfully. Verifies parsing of complex nested array blocks and message objects (Claude and Gemini formats).
*   **Divergences**: None.
*   **Status**: `verified`

### Flow 3: Request Routing & MCP Dispatcher (T7)
*   **Verification Command**: `cargo nextest run --test test_client_server_auto_routing` and code review.
*   **Expected Behavior**:
    - Axum router binds endpoints under `/v1/chat/completions`, `/v1/mcp/call`, and `/v1/episodes`.
    - Authorization header `X-Mythrax-Token` is verified on incoming gateway requests.
    - CLI/Client auto-detects active daemon port and falls back to server mode if offline.
*   **Observed Behavior**:
    - `test_client_server_auto_routing_detection` passed successfully. Verifies client automatically routes requests through the daemon port when active, and falls back to opening direct SurrealDB KV connections when the daemon is offline.
    - In `api.rs`, the router maps routes cleanly and executes token validation.
*   **Divergences**: None.
*   **Status**: `verified`

### Flow 4: Memory Co-existence & Retrieval Router (T8)
*   **Verification Command**: `cargo nextest run --test test_sigmoid_gated_search` and `cargo nextest run --test test_verbatim_floor`.
*   **Expected Behavior**:
    - Setting `include_episodes: false` excludes raw logs from the prompt.
    - Similarity scores are sigmoid-gated to filter poor matches.
    - Decayed episodes remain retrievable under a demoted rank (floor).
*   **Observed Behavior**:
    - `test_sigmoid_gated_retrieval_formula` passed successfully. Verified that similarity scores mapped below `0.55` yield a near-zero gate, while scores above `0.65` yield a gate near `1.0`.
    - `decayed_episode_still_retrievable_but_demoted` passed successfully. Verified that episodes with utility `< 10.0` are marked as archived instead of deleted, and their search relevance scores are penalised with a `0.4` multiplier, demoting them in search rankings (verbatim floor).
    - `raptor_summary_is_additive_not_replacement` verifies that compacted episodes remain in the database alongside synthesized permanent wiki nodes, preserving raw history.
*   **Divergences**: None.
*   **Status**: `verified`

### Flow 5: Background Sweeps & Compaction Recovery (T9)
*   **Verification Command**: `cargo nextest run --test test_abandoned_session_sweep`.
*   **Expected Behavior**:
    - The compactor detects sessions idle for >10m with modified transcript files.
    - Ingests trailing turns, updates `_last_swept_at` timestamp, and leaves the registry key registered.
    - Deletes registry keys if the transcript file is missing.
*   **Observed Behavior**:
    - `test_abandoned_session_sweep_lifecycle` passed successfully. Verified the compactor detects sessions idle for >10m, compares file metadata modified times with `_last_swept_at` in STM, and calls `mine_transcript` to import new trailing transcript turns.
    - If the transcript file is deleted or missing, the compactor deletes the registered session path from the STM registry to prevent leakage.
*   **Divergences**: None.
*   **Status**: `verified`
