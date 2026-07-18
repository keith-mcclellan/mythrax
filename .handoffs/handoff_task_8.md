# Handoff Contract: Task 8 (Cloud Token Back-Pressure)

## Objective
Implement token back-pressure mechanisms to protect cloud brain quota and ensure resume/recovery capability.

## Files to Modify
- `mythrax-core/src/cognitive/synthesis.rs`
- `mythrax-core/src/hooks/precompact.rs`
- `mythrax-core/src/daemon.rs`
- `mythrax-core/tests/test_task8.rs` (new test file)

## Requirements
1. **Red (Test)**:
   Write tests verifying:
   - `test_checkpoint_resume`: Verify that Phase A bulk ingestion records its last processed index to `bootstrap_checkpoint.json` and resumes from it if interrupted.
   - `test_quota_exhaustion_hibernation`: Mock `routed_completion` to return timeouts / errors 3 times in a row, and verify that the daemon enters hibernation (sleeps for `MYTHRAX_QUOTA_RETRY_SECS` / stays idle) rather than crashing or continuously loop-failing.

2. **Green (Code)**:
   - **Checkpoint Persistence**:
     - During bulk ingestion in `precompact.rs`, persist the current processing state/index into `{vault_root}/.mythrax/bootstrap_checkpoint.json`.
     - When starting bulk ingestion, check if this file exists and contains a valid checkpoint. If so, resume streaming transcript episodes from the recorded index.
   - **Rate Limiting**:
     - Implement a rate limiter around cloud calls (`routed_completion`). Limit requests to `MYTHRAX_CLOUD_RPM` (default 30 RPM) using a thread-safe interval guard.
   - **Cooldown Sleeps**:
     - Add a configurable cooldown sleep (`MYTHRAX_PHASE_COOLDOWN_SECS`, default 300s) between Phase A (ingestion) and Phase B (synthesis) when triggered by bulk ingestion.
   - **Hibernation State**:
     - Implement a retry tracker. If 3 consecutive cloud completions fail or time out, enter a hibernation state (e.g., set an internal status and sleep/wait for `MYTHRAX_QUOTA_RETRY_SECS` default 3600s). Emit structured logs with `phase` and `progress`.
