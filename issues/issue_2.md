# Agent Orchestration: Unbounded Recursion via File Watcher Coalescing

**Finding** -> The vault watcher coalesces file edit events over a 500ms sliding window (e.g., `Instant::now() + Duration::from_millis(500)` in `vault/watcher.rs`).
**Current Assumption** -> Suppressing `reference/` and `*.tmp` paths (as seen in `vault/watcher.rs`) prevents cascading dreaming passes.
**Attack Scenario** -> An adversarial prompt instructs an agent to make trivial, staggered edits (e.g., every 600ms) to a monitored source file (e.g., `src/rogue.rs`). This continuously triggers the watcher beyond the 500ms coalescing window, invoking the agent into an infinite loop of cognitive synthesis without hitting depth boundaries.
**Blast Radius** -> Denial of service, unbounded token usage, CPU bottlenecking, and disk thrashing.
**Recommended Structural Change** -> Implement a token bucket rate-limiter for cognitive triggers and strict depth boundaries/cooldowns on watcher-induced agent invocations.


**Labels**: `architecture-review`, `adversarial`

*Note: Do not close this issue without a documented architectural decision record (ADR) response.*
