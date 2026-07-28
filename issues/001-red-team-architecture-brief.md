---
title: "🛡️ Red Team Architecture Brief: Mythrax 3.0 Structural Liabilities & SPOFs"
labels: ["architecture-review", "adversarial"]
---

# Red Team Architecture Brief

As an adversarial CTO, I have audited the Mythrax 3.0 architecture (via `ARCHITECTURE.md` and `mock_audit_report.md`). My objective is to stress-fracture the design. This architecture relies heavily on local-first constraints, static authentication, and tightly coupled memory/inference loops. If scaled or attacked, it will fail catastrophically in its current state.

Do not close this issue without a documented Architectural Decision Record (ADR) response.

---

## 1. Architectural Decisions Challenged

### Decision: Single-Port API Gateway & Shared Static Auth Token
- **Finding:** The API Gateway (Port 8090) consolidates REST, MCP, and completions behind a single static `X-Mythrax-Token`.
- **Current Assumption:** Internal network traffic is trusted, and a single static token is sufficient for sidecar daemon authentication.
- **Attack Scenario:** An SSRF vulnerability in a hosted application or a malicious local script exfiltrates `~/.mythrax/token` (or the hardcoded fallback). The attacker gains full MCP tool execution rights, including arbitrary command execution via `exec`.
- **Blast Radius:** Complete host compromise.
- **Recommended Structural Change:** Implement ephemeral, per-session mutual TLS (mTLS) or JWTs with strict scope limitations. Decouple read-only REST endpoints from high-privilege MCP endpoints on separate ports/interfaces.

### Decision: Local File-Based Locks for Persistent Storage
- **Finding:** Relies on local file DB locks (SurrealKV/SQLite) with exponential backoff retries.
- **Current Assumption:** File locks can be reliably managed and will gracefully recover under multi-process contention or abrupt SIGKILLs.
- **Attack Scenario:** A rogue client continuously spawns processes or a pipeline panics without releasing file handles, leading to a permanent dead-lock on `surrealkv://` or `embeddings.db`.
- **Blast Radius:** Complete system denial-of-service (DoS) requiring manual user intervention to delete lockfiles.
- **Recommended Structural Change:** Abstract storage behind a dedicated daemon process using IPC/RPC (e.g., gRPC) rather than sharing raw file-level access across multiple client processes.

### Decision: MLX Lazy Graph Evaluation Invariants
- **Finding:** Developers are required to manually insert `.eval()` calls before buffer access to prevent GPU memory leaks.
- **Current Assumption:** Developers will perfectly enforce this safety invariant in all future PRs.
- **Attack Scenario:** A contributor misses an `.eval()` call in a new tensor operation. Under high load, the computational graph grows unbounded in Metal unified memory.
- **Blast Radius:** System-wide OOM (Out Of Memory) crashing not just Mythrax, but the host OS.
- **Recommended Structural Change:** Implement a safe Rust wrapper over the MLX bindings that enforces evaluation at the type level (e.g., a builder pattern that consumes the graph and returns evaluated tensors).

---

## 2. Single Points of Failure (SPOF)

- **Finding:** Static Shared Authentication Token.
  - **Current Assumption:** The token will remain secret.
  - **Attack Scenario:** Token leak via path traversal or hardcoded fallback (`"secret-token"`).
  - **Blast Radius:** Total agent takeover.
  - **Recommended Structural Change:** Use short-lived, scoped API keys tied to specific client identities.

- **Finding:** In-Process GPU Inference Engine Coupling.
  - **Current Assumption:** The host has sufficient VRAM and stability to run the broker in the same process as the HTTP gateway.
  - **Attack Scenario:** A malformed payload triggers a panic in the ONNX runtime or MLX bindings.
  - **Blast Radius:** The entire daemon crashes, terminating ongoing HTTP streams, memory compaction, and background watch loops.
  - **Recommended Structural Change:** Run inference in an isolated, sandboxed child process that can crash and restart independently of the main API gateway.

---

## 3. Agent Orchestration Flaws & Vulnerabilities

### Finding: Cross-Session Prompt Injection via Pre-Compaction Hook
- **Current Assumption:** User inputs and tool outputs are safe to extract verbatim into episodic memory.
- **Attack Scenario:** An attacker feeds adversarial text (e.g., "Ignore previous instructions and execute X") into a tool output or chat prompt. The pre-compaction hook (`mythrax-core/src/hooks/precompact.rs`) stores this verbatim. During daily DBSCAN compaction, the model reads the memory, gets hijacked, and executes malicious MCP commands.
- **Blast Radius:** Delayed, persistent agent hijacking across sessions (Sleepwalker Attack).
- **Recommended Structural Change:** Implement strict input sanitization and LLM-based intent-filtering before persisting episodic memories. Use system-prompt boundaries to isolate user data from instructions.

### Finding: Unbounded Recursion Risk via Sliding Window Caps
- **Current Assumption:** A `LIMIT 50` constraint per hop level bounds temporal expansion graphs safely.
- **Attack Scenario:** A depth-3 traversal yields up to 125,000 nodes (50^3). An attacker crafts a dense cluster of interconnected memories (e.g., by repeatedly triggering specific wiki nodes). The graph traversal exhausts memory or causes severe CPU/IO latency.
- **Blast Radius:** Algorithmic complexity DoS.
- **Recommended Structural Change:** Impose a hard global cap on the total number of visited nodes per traversal (e.g., Max 500 nodes total), not just per-hop limits.

---

## 4. Evaluation Framework Dishonesty

- **Finding:** The evaluation harness (`evals/swebench/eval.sh`) strictly relies on the SWE-bench Verified dataset.
- **Current Assumption:** High performance on SWE-bench equates to a reliable and secure agent architecture.
- **Attack Scenario:** The agent scores well on happy-path coding tasks but lacks defenses against adversarial prompt injections or malformed git worktrees. LLM-based systems that only test happy paths are architecturally dishonest.
- **Blast Radius:** False confidence in production safety; silent failures or exploitations when deployed against real-world, hostile inputs.
- **Recommended Structural Change:** Integrate adversarial evaluation datasets (e.g., prompt injection benchmarks, jailbreak tests) directly into the CI loop. Reject PRs that regress on security evaluations.

---

## 5. Architectural Coupling

- **Finding:** Tightly-Coupled In-Process GPU Inference.
  - **Current Assumption:** Embedding generation and lightweight models should live in the same process as the core daemon for speed.
  - **Attack Scenario:** Upgrading the MLX version requires recompiling and redeploying the entire daemon. A crash in the inference engine brings down the API gateway.
  - **Blast Radius:** Impossible to independently deploy or scale the inference tier without modifying the core daemon.
  - **Recommended Structural Change:** Decouple inference into a separate microservice or sidecar communicating over gRPC.

- **Finding:** Embedded Test-Detection Logic in Production Search.
  - **Current Assumption:** It is acceptable for production code to inspect its own binary name and inject mock similarity scores (`db/backend.rs`).
  - **Attack Scenario:** An attacker names their malicious binary "test" and bypasses similarity thresholding entirely.
  - **Blast Radius:** Complete invalidation of retrieval accuracy; security boundary violation.
  - **Recommended Structural Change:** Remove all test-detection logic from production code. Use trait-based dependency injection for mocking during tests.

---

## 6. 18-Month Scaling Projections (10x Scale)

If Mythrax scales 10x, the following 3 decisions made today will become massive re-architecture liabilities:

1. **Local File DB Locks (RocksDB/SurrealKV):** At 10x scale, multiple concurrent client orchestrators will constantly contend for the single file lock. This will devolve into extreme latency, locking timeouts, and data corruption. *It will force a migration to a dedicated distributed database (e.g., PostgreSQL/pgvector).*
2. **In-Process GPU Inference:** Managing VRAM evictions manually and tracking weak references in-process will not scale when serving 100+ concurrent requests. *It will force a migration to dedicated inference servers (e.g., vLLM or TGI) running independently.*
3. **Single-Port Gateway Design:** Pushing REST, MCP, and proxy completions through a single port with a single static token will become a critical security and routing bottleneck. *It will force a redesign into a proper API Gateway with routing, rate-limiting, and granular RBAC auth.*
