# Red Team Architecture Brief

## Executive Summary
This brief is written from the perspective of an adversarial CTO challenging the core architectural assumptions in Mythrax 2.0. The objective is to identify single points of failure, scaling bottlenecks, boundary violations in agent orchestration, missing adversarial evaluations, and tight coupling across modules.

---

## 1. Finding: Single-Port API Gateway (Port 8090) Single Point of Failure
- **Current Assumption:** Consolidating REST, MCP, and proxy completions onto a single port (8090) with a shared static `X-Mythrax-Token` auth boundary simplifies the API and makes auto-spawn detection easy.
- **Attack Scenario:** A malicious agent or a compromised client floods the single port with large payloads or malformed MCP commands. Additionally, if the single static token is leaked or cracked, an attacker gains complete administrative, memory, and cognitive graph manipulation rights.
- **Blast Radius:** Total system compromise. The gateway cannot gracefully degrade; if the unified router fails under load, all REST, MCP, and proxy capabilities crash simultaneously, taking down the entire sidecar intelligence daemon.
- **Recommended Structural Change:** Decouple administrative endpoints from memory/inference endpoints. Implement separate network listeners and distinct RBAC/token mechanisms for admin control vs. standard agent operations. Introduce rate limiting and distinct worker pools to isolate gateway paths.

---

## 2. Finding: In-Process Fallback and Model Broker Tight Coupling
- **Current Assumption:** Creating dummy/mock LLM engines (e.g., `dummy`, `fallback-cpu-model`) silently within the production model broker upon acquisition failure maintains system uptime.
- **Attack Scenario:** A rapid sequence of complex semantic requests exhausts VRAM, causing model acquisition to fail. The system silently falls back to a `fallback-cpu-model` which produces hallucinated or mocked answers. An adversarial prompt exploits this by intentionally crashing the VRAM (e.g., extremely long context injection) and then hijacking the degraded/mocked model that lacks guardrails.
- **Blast Radius:** Silent data corruption in memory clustering and cognitive schedules. Memory/knowledge synthesis will generate garbage rules and nodes that get permanently written into the database.
- **Recommended Structural Change:** Remove all silent mocked fallbacks in production. Propagate model acquisition errors to the caller. Implement circuit breakers and queueing with explicit backpressure, enforcing a hard failure rather than graceful degradation to garbage.

---

## 3. Finding: Prompt Injection Risks in "Verbatim" Pre-Compaction Hooks
- **Current Assumption:** Ingesting verbatim flat string or array-of-block transcripts (including raw tool results) and indexing them directly into episodic memory is safe because it preserves context.
- **Attack Scenario:** An external input (e.g., from a web scrape or untrusted user prompt) contains malicious prompt injection payloads disguised as tool outputs. Since the pre-compaction hook parses these verbatim without sanitization, the injection is stored permanently. When DBSCAN clusters these memories and the Arbor HTR loop synthesizes rules via the LLM, the injection executes, potentially altering permanent wisdom rules or system configurations.
- **Blast Radius:** Complete pollution of the permanent `wiki_node` and wisdom rule storage, leading to unbounded recursion where the system continuously feeds itself poisoned context during daily dreaming cycles.
- **Recommended Structural Change:** Introduce a strict sanitization and boundary-enforcement layer between raw episodic ingestion and the dreaming compactor. All retrieved verbatim memories must be wrapped in strong system boundaries before being passed back into LLM context for synthesis.

---

## 4. Finding: Thread-Safe WAL and SQLite/RocksDB Content Contention
- **Current Assumption:** A 500ms sliding window and a retry loop with a backoff (up to 9 or 10 attempts) can resolve persistent file lock contention between the WAL actor and concurrent DB access.
- **Attack Scenario:** Under high-concurrency ingestion (e.g., multiple agents parallel-processing a massive codebase), the database lock retry loop continuously exhausts its 9 attempts. The WAL falls behind, and simultaneous read/write operations experience cascading timeouts.
- **Blast Radius:** Data loss during abrupt power failures because the WAL couldn't commit, and severe API latency rendering the daemon unresponsive to clients. No graceful degradation path exists for database lock exhaustion.
- **Recommended Structural Change:** Replace the file-lock retry loop with a dedicated single-writer connection pool or message queue architecture for all SQLite/RocksDB writes, completely eliminating cross-process file lock contention.

---

## 5. Finding: Non-Adversarial Happy-Path Eval Framework
- **Current Assumption:** The `evals/swebench/eval.sh` script testing against standard SWE-bench datasets is sufficient to validate agent capabilities and architectural robustness.
- **Attack Scenario:** The LLM and memory synthesis modules are highly susceptible to malicious inputs (e.g., adversarial prompt manipulation, context window stuffing). The current eval framework only tests functional "happy paths" (standard bug fixes) and completely ignores adversarial boundary testing. An attacker injects hidden text into a dataset, which the agent blindly trusts and executes.
- **Blast Radius:** The architecture is fundamentally dishonest about its resilience. Deploying it in a production environment exposes the system to trivial prompt-injection data exfiltration.
- **Recommended Structural Change:** Integrate adversarial evaluation suites (e.g., PromptInject, Garak) directly into `evals/`. Fail the build if the agent complies with out-of-scope instructions or leaks the static auth token.

---

## 6. Finding: 18-Month 10x Scale Projections
If the system scales 10x in request volume and data size, the following three architectural decisions will become immediate re-architecture projects:

1. **Sigmoid Gated Search Indexing in SQLite/SurrealKV:**
   - At 10x scale, running continuous semantic vector search (cosine similarity + Sigmoid gating formula) entirely within a local KV/SQL layer will grind to a halt. It requires a dedicated vector database (e.g., Qdrant, Milvus) to handle millions of episodic vectors efficiently.
2. **500ms File Watcher Coalescing (Obsidian Vault):**
   - Watching tens of thousands of vault files with a static 500ms sliding window will cause excessive CPU thrashing and dropped events. It must be replaced by an event-driven queueing system (e.g., Redis PubSub or Kafka) rather than raw filesystem polling.
3. **Sequential Model Eviction & Swapping (VRAM Management):**
   - Swapping models out of VRAM sequentially on a single machine will create massive latency bottlenecks for concurrent multi-agent workloads. The system will need a distributed model routing layer across multiple GPU nodes (e.g., vLLM or Ray Serve) instead of a monolithic local broker.
