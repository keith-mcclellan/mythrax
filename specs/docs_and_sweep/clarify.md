# Clarify: Documentation Update & Background Transcript Sweep

## Restated Request
1. Map out the step-by-step data flows through Mythrax 2.0.
2. Build a plan to test each step individually.
3. Update [ARCHITECTURE.md](file:///Users/keith/Documents/mythrax/ARCHITECTURE.md) and [DEVELOPMENT.md](file:///Users/keith/Documents/mythrax/DEVELOPMENT.md) in the repository root.
4. Implement a background transcript sweep in the dreaming coordinator to capture unmined trailing turns from abandoned sessions, and add automated tests to verify this behavior.

## Known Facts
* Mined episodes do not contain a `session_id` column due to SurrealDB table SCHEMAFULL constraints.
* Session-specific episodes are chained together via `followed_by` edges using the `_last_episode_id` key in the Short-Term Memory (STM) table.
* The pre-compaction hook transcript miner (`mine_transcript`) saves raw episodes with `processed_in_dream = false`.
* The dreaming compactor gathers all unprocessed episodes scope-wide via `SELECT * FROM episode WHERE processed_in_dream = false;`.
* Abandoned sessions are sessions that have no activity (STM writes or completions calls) for more than 10 minutes.

## Assumptions
* Spawning a new agent or restarting a task registers the new transcript path in STM under `_transcript_path`.
* Clean sweep means removing the `_transcript_path` key from STM upon successful mining to avoid re-checking/re-mining the file in future dreaming runs.

## Tradeoffs
* Background sweeps require file reads (`BufReader` parsing JSONL) during the idle dreaming cycle. This is acceptable as dreaming is gated on user inactivity (10 minutes of idle time).

## Blocking Questions
*All blocking design questions have been resolved via the `/grill-me` interview: architecture files will be overwritten directly, the sweep logic and tests will be implemented now, and STM key cleanup will involve deleting the key.*
