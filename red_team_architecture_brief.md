# Red Team Architecture Brief

## 1. API Gateway Single Point of Failure
**Finding** -> The Single-Port API Gateway multiplexes REST, MCP, and OpenAI completions into a single router on port 8090, configured via `MYTHRAX_DAEMON_PORT` in `main.rs` and `cli.rs`.
**Current Assumption** -> The unified router will handle all administrative, memory, MCP, and completions proxy endpoints fairly without resource starvation.
**Attack Scenario** -> An adversary or runaway agent floods the 8090 port with heavy model context payloads or search queries. The single port blocks, preventing administrative control (`/v1/daemon/stop` endpoint in `api.rs`) from being successfully processed, which leads to total system lockout.
**Blast Radius** -> Complete system lockout and loss of control over autonomous agents.
**Recommended Structural Change** -> Decouple the administrative control plane (stop commands, config) to an isolated port or process separated from the data plane.

## 2. Agent Orchestration: Unbounded Recursion via File Watcher Coalescing
**Finding** -> The vault watcher coalesces file edit events over a 500ms sliding window (e.g., `Instant::now() + Duration::from_millis(500)` in `vault/watcher.rs`).
**Current Assumption** -> Suppressing `reference/` and `*.tmp` paths (as seen in `vault/watcher.rs`) prevents cascading dreaming passes.
**Attack Scenario** -> An adversarial prompt instructs an agent to make trivial, staggered edits (e.g., every 600ms) to a monitored source file (e.g., `src/rogue.rs`). This continuously triggers the watcher beyond the 500ms coalescing window, invoking the agent into an infinite loop of cognitive synthesis without hitting depth boundaries.
**Blast Radius** -> Denial of service, unbounded token usage, CPU bottlenecking, and disk thrashing.
**Recommended Structural Change** -> Implement a token bucket rate-limiter for cognitive triggers and strict depth boundaries/cooldowns on watcher-induced agent invocations.

## 3. Ephemeral State Persistence Data Corruption
**Finding** -> DBSCAN clustering ephemeral state is stored in SurrealDB's `pipeline_cluster` table (defined in `schema.rs` and accessed in `crud_operations.rs`).
**Current Assumption** -> The process will safely execute cleanup operations during normal operation or shutdown.
**Attack Scenario** -> A hard OOM kill (SIGKILL) bypassing tokio's graceful shutdown leaves orphaned clustering data in the persistent `pipeline_cluster` SurrealDB table. Subsequent runs hallucinate connections to stale cluster states, leading to memory contamination.
**Blast Radius** -> Corruption of cognitive synthesis and cross-contamination of memory clusters.
**Recommended Structural Change** -> Store ephemeral clustering state entirely in memory or an explicitly ephemeral store (like Redis) rather than the persistent SurrealKV.

## 4. Prompt Injection via Verbatim Episodic Memory
**Finding** -> The system preserves episodic memories verbatim, storing them via the SurrealDB database and formatting them with `vault::watcher::format_episode_markdown`.
**Current Assumption** -> Retrieval context is inherently trustworthy and does not contain malicious instructions.
**Attack Scenario** -> An adversary embeds a prompt injection in a downloaded file or external webpage. Mythrax ingests it verbatim into an episode. When retrieved via hybrid search, the payload commandeers the LLM.
**Blast Radius** -> Agent takeover, potential unauthorized code execution via agent tools.
**Recommended Structural Change** -> Introduce an LLM-based or deterministic sanitation layer prior to retrieving and injecting memories into the prompt buffer.

## 5. Architecturally Dishonest Evals (Happy-Path Only)
**Finding** -> The `evals/` framework only implements the SWE-bench Verified coding dataset (`evals/swebench/eval.sh`), evaluating code completion tasks without adversarial checks.
**Current Assumption** -> Code correctness on SWE-bench validates the architecture's fitness and safety.
**Attack Scenario** -> The system passes all SWE-bench tests but fails instantly when presented with a jailbreak payload or prompt injection because no adversarial evals exist in the test suite.
**Blast Radius** -> False confidence in system security; highly vulnerable to attacks in the wild.
**Recommended Structural Change** -> Integrate adversarial evaluation frameworks into `evals/` and enforce a failing CI pipeline if jailbreaks or scope boundary violations succeed.

## 6. Daemon Crash via In-Process MLX Engine
**Finding** -> Lightweight model tiers run in-process using the Metal GPU backend via `InProcessMlxEngine` (defined in `llm/mod.rs`).
**Current Assumption** -> In-process inference via MLX bindings is memory-safe and stable.
**Attack Scenario** -> A malformed tensor or out-of-bounds array access in the MLX bindings causes a segmentation fault.
**Blast Radius** -> The entire Mythrax daemon, including the SurrealDB memory backend, crashes instantly without graceful shutdown, violating safety safeguards.
**Recommended Structural Change** -> Isolate model execution into a separate sidecar process connected via IPC/gRPC.


## 18-Month Architecture Projections (Re-Architecture Projects)
1. **SurrealKV Write Bottleneck**: At 10x scale, multiple agents reading/writing simultaneously will exhaust the SurrealKV local store connection pool, forcing migration to a true distributed database engine.
2. **500ms File Watcher Coalescing**: Scanning a 10x larger codebase every 500ms will become a CPU bottleneck; requires migrating to localized OS event handlers without recursive directory sweeps.
3. **In-Process Inference Extraction**: As models grow and context windows expand, OOM risks in `InProcessMlxEngine` will force splitting the monolith into distinct Memory and Inference microservices to protect the database layer.
