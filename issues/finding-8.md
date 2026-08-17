---
title: "Finding 8: 18-Month Re-architecture Project - Synchronous Fallback in Async Runtime"
labels: ["architecture-review", "adversarial"]
---

**Finding**: The architecture handles synchronous legacy functions via the Strangler pattern or `tokio::task::block_in_place` bridges when async bubbling is blocked by sync traits.

**Current Assumption**: The assumption is that `block_in_place` provides a safe escape hatch for legacy synchronous code without starving the Tokio async executor, and that the volume of these calls will remain low.

**Attack Scenario**: If the system scales 10x, the frequency and duration of these synchronous bridges will increase. `block_in_place` forces the Tokio runtime to move the current task to a blocking thread and potentially spin up new worker threads to maintain concurrency. Under heavy load (or adversarial load generating complex sync operations), this can lead to thread pool exhaustion, excessive context switching, and a complete stall of the async executor.

**Blast Radius**: Total system unresponsiveness. The single-port API gateway will stop accepting new connections, and background tasks will freeze.

**Recommended Structural Change**: The transition away from synchronous traits must be completed. Relying on `block_in_place` as a permanent architectural bridge is a liability. The system will need a full rewrite of legacy synchronous traits to native async/await, or isolation of synchronous legacy code into entirely separate, dedicated OS processes communicating via IPC, removing them from the Tokio runtime entirely.
