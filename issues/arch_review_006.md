# Red Team Architecture Brief

## Finding
18-Month Re-architecture Liability: Monolithic Tokio Runtime

## Current Assumption
A single shared tokio runtime is sufficient for handling HTTP proxying, background DBSCAN clustering, file watching, and git worktree verification.

## Attack Scenario
As the system scales 10x, CPU-intensive tasks (like DBSCAN epsilon calibration or HTR verification loops) will starve the async executor, delaying HTTP completions and API responses. Even with non-blocking async primitives, the sheer volume of parallel tasks in a single process will hit OS-level limits or cause latency spikes.

## Blast Radius
System-wide latency degradation. Time-sensitive agent API calls time out.

## Recommended Structural Change
Partition the architecture into specialized workers. Separate the HTTP API Gateway, the Background Compaction Workers, and the File/Git Sync Daemons into distinct runtimes or separate deployable binaries.
