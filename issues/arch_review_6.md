# Streaming-to-Disk cognitive pipeline tightly coupled to Obsidian Vault

**Labels**: `architecture-review`, `adversarial`

## Finding
The Streaming-to-Disk Cognitive Pipeline strictly writes canonical outputs to Obsidian Vault markdown and JSON files, creating an I/O bottleneck.

## Current Assumption
File system I/O latency (specifically generating and writing many small Markdown/JSON files) will not bottleneck the parallel cognitive tasks or memory stream.

## Attack Scenario
At 10x scale, rapid streaming of wiki nodes and wisdom rules generates thousands of I/O operations per second (IOPS). The 500ms file watcher coalescing queue overflows, disk latency spikes, and the Tokio runtime becomes I/O blocked, preventing the API gateway from processing incoming requests.

## Blast Radius
Severe performance degradation. The memory compaction stream falls behind real-time ingestion, resulting in stale agent context and delayed insights.

## Recommended Structural Change
Decouple the Obsidian Vault sync into an asynchronous, batched, low-priority background process. Treat the SurrealDB graph as the sole source of truth and only synchronize to disk periodically or on-demand, rather than inline with cognitive artifact generation.
