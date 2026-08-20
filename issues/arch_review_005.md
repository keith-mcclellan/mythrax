# Red Team Architecture Brief

## Finding
Single Point of Failure: 500ms Obsidian Vault Watcher Coalescing

## Current Assumption
A 500ms sliding window is sufficient to coalesce file edit events and prevent write amplification or lock contention when generating cognitive artifacts.

## Attack Scenario
Under massive concurrent load (e.g., generating thousands of AST symbols or rapid wiki node updates), the 500ms window aggregates an enormous batch of events. If the subsequent processing (e.g., `sync_workspace_docs_to_vault`) is I/O bound or blocks briefly, the watcher queue backs up. An adversary can intentionally spam file writes to exhaust memory or trigger the 5-second graceful shutdown timeout, causing data loss.

## Blast Radius
Memory exhaustion, skipped file syncs, and potential data loss during shutdown.

## Recommended Structural Change
Implement backpressure mechanisms and dynamic window sizing. If the queue size exceeds a threshold, reject new events or spill them to disk instead of relying purely on a temporal window.
