# Red Team Architecture Brief

**Persona:** Adversarial CTO
**Focus:** Architectural stress-testing, vulnerability identification, and 18-month scalability bottlenecks for Mythrax 3.0.

This brief challenges the documented architectural decisions in `ARCHITECTURE.md` and highlights where the system will fail under load, adversarial inputs, or changing requirements.

---

## 1. Prompt Injection Vulnerability in Episodic Memory
- **Finding:** The pre-compaction hook in `mythrax-core/src/hooks/precompact.rs` extracts tool results and user inputs verbatim into episodic memory without sanitization.
- **Current Assumption:** Agent orchestration and tools only process safe internal data; verbatim extraction is a benign operation.
- **Attack Scenario:** A malicious external input or manipulated tool output contains a prompt injection payload. It is extracted verbatim, poisoning the cross-session episodic memory. When future agent loops or the DBSCAN compactor process this memory, the injected payload hijacks the cognitive pipeline.
- **Blast Radius:** Complete compromise of downstream cognitive models and agents. Exfiltration of data, bypass of scope boundaries, and manipulation of agent actions across sessions.
- **Recommended Structural Change:** Introduce a strict sanitization, encoding, and validation layer before writing raw input or tool output to episodic memory. Segregate raw inputs from synthesized context, and enforce strict system-prompt boundaries.

## 2. Unbounded Recursion / DoS in Temporal Expansion Graph
- **Finding:** `LIMIT 50` constraints on temporal expansion graph traversals (e.g., in `db/crud_operations.rs`) are applied per hop level, leading to unbounded recursion risk.
- **Current Assumption:** Bounded pagination per hop sufficiently prevents unbounded recursion and memory explosion.
- **Attack Scenario:** An attacker triggers the creation of dense, highly interconnected memory clusters. A depth-3 traversal yields $50 \times 50 \times 50 = 125,000$ nodes. The agent attempts to process this massive context in a single sweep, causing severe CPU starvation and memory exhaustion.
- **Blast Radius:** Denial of Service (DoS) for the entire Mythrax daemon due to memory explosion, stalling processing and crashing the node.
- **Recommended Structural Change:** Implement an absolute global cap on the total number of traversed nodes across all hops (e.g., max 500 nodes total). Apply graph decay algorithms to prioritize nodes and prune the search space early.

## 3. Single Point of Failure in Unified API Gateway & Shared Auth
- **Finding:** The Single-Port API Gateway (port 8090) operates with a shared static auth token (`X-Mythrax-Token`).
- **Current Assumption:** Consolidating routes onto a single port with a shared token simplifies architecture without compromising security.
- **Attack Scenario:** An attacker leaks or brute-forces the static token, gaining full control over administrative, routing, and memory endpoints. Furthermore, a crash in this single API Gateway process brings down all decoupled endpoints simultaneously.
- **Blast Radius:** Total system compromise and data exfiltration. Complete downtime across all functions with no graceful degradation path.
- **Recommended Structural Change:** Decouple administrative and MCP endpoints. Replace the single static token with dynamic, cryptographically signed JWTs scoped by role and subsystem.

## 4. Deceptive Evals and Lack of Adversarial Input Testing
- **Finding:** The evaluation framework in `evals/swebench/` solely focuses on verified, functional "happy paths".
- **Current Assumption:** High scores on SWE-bench Verified accurately reflect system robustness and safety.
- **Attack Scenario:** The system encounters malformed or intentionally deceptive input (e.g., obfuscated prompt injections, manipulated codebase structures) in production. It processes these naively, bypassing boundaries.
- **Blast Radius:** System brittleness, unexpected catastrophic failures, and vulnerability to non-cooperative inputs, undermining benchmark assurances.
- **Recommended Structural Change:** Integrate adversarial red-teaming benchmarks, boundary testing, and fuzzing directly into the `evals/` suite. Explicitly measure failure handling and boundary enforcement.

## 5. Architectural Coupling and Crash Risk via In-Process MLX Engine
- **Finding:** The In-Process Engine loads models natively into the daemon's process memory using the MLX Metal GPU backend.
- **Current Assumption:** Loading the MLX model natively provides optimal latency and simplifies deployment.
- **Attack Scenario:** A missing `.eval()` call, malformed prompt, or large context window causes an OOM error or panic within the native MLX C++ bindings. This native crash abruptly kills the entire daemon.
- **Blast Radius:** Instant crash of the entire Mythrax daemon, terminating all active client sessions, compactions, watchers, and proxies. No graceful degradation.
- **Recommended Structural Change:** Decouple MLX model execution into a distinct, isolated worker process communicating over IPC or gRPC. If the worker crashes, the main daemon survives and can restart it or fall back to external APIs.

## 6. 18-Month Scaling Bottlenecks (10x Scale Projection)
- **Finding:** Three critical decisions will become re-architecture projects at 10x scale.
- **Current Assumption:** The hybrid architecture and streaming pipeline will scale 10x smoothly.
- **Attack Scenario / Bottlenecks:**
  1. **SQLite Embedding Cache (`embeddings.db`) I/O Locks:** Concurrent reads/writes during massive vector operations will cause severe lock contention.
  2. **Single-Port API Gateway Contention:** A shared static `reqwest::Client` will exhaust file descriptors under sustained proxying.
  3. **Streaming-to-Disk Markdown Pipeline:** Writing heavy Obsidian Vault markdown files directly to disk for every cognitive sync will choke on filesystem I/O.
- **Blast Radius:** Severe latency degradation, I/O bottlenecks, dropped API requests, and failing cognitive syncs.
- **Recommended Structural Change:**
  1. Replace SQLite embedding cache with a dedicated distributed vector database.
  2. Implement connection pooling and backpressure for API requests.
  3. Decouple cognitive syncs from raw filesystem writes; use an async message queue or robust document database for interim storage.
