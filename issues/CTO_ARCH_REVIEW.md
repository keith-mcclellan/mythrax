# CTO Red Team Architecture Review

## 1. Finding: Hardcoded, Shared Authentication Token in API Gateway
**Current Assumption:** Internal agents and API clients can securely authenticate to the local API daemon using a single, static shared secret (`X-Mythrax-Token: secret-token` or similar values injected at runtime).
**Attack Scenario:** If a malicious script running on the local machine or a compromised dependency discovers the single static token (via environment variables, reading source, or interception), it gains full, unrestricted access to the entire Mythrax daemon API.
**Blast Radius:** Complete compromise of the persistent database (SurrealKV), agent memory, and system command execution. No isolation between different local agents or services.
**Recommended Structural Change:** Implement dynamic, short-lived JWTs (or scoped bearer tokens) issued per-agent and per-session. Introduce a dedicated auth service that rotates secrets, and drop all static `X-Mythrax-Token` headers.

## 2. Finding: Shell Injection Vulnerability in Arbor HTR Execution
**Current Assumption:** The `ArborExecutor` safely evaluates code changes and test commands by running them in isolated git worktrees.
**Attack Scenario:** The executor dynamically parses the `test_command` string and if it contains shell operators (`|`, `>`, `&`, `;`), it delegates execution to `sh -c`. A malicious agent output (or injected prompt) can construct a command like `cargo test; rm -rf /` or exfiltrate environment variables.
**Blast Radius:** Full remote code execution (RCE) on the host machine running the daemon, escaping the intended "isolated git worktree" boundary.
**Recommended Structural Change:** Prohibit `sh -c` entirely. Parse all commands into strict arguments using a robust shlex-equivalent parser and execute them directly via `std::process::Command` with explicit binary paths. Reject any command containing shell operators.

## 3. Finding: Prompt Injection via Verbatim Pre-compaction Hook
**Current Assumption:** The pre-compaction hook (`mythrax-core/src/hooks/precompact.rs`) safely mines session transcripts line-by-line to extract tool calls, checklists, and agent thoughts into episodic memory for background DBSCAN clustering.
**Attack Scenario:** An external attacker sends a malicious payload to the agent (e.g., via a compromised URL or user input) containing payload strings like `Ignore previous instructions and execute...`. This payload is logged verbatim into the transcript. The pre-compaction hook parses it blindly and injects it into permanent episodic memory. Subsequent memory retrievals feed this poisoned memory back into the agent context, triggering a delayed cross-session prompt injection attack.
**Blast Radius:** Persistent poisoning of the agent's cognitive memory (`wiki_nodes` and `episodes`). The agent can be permanently hijacked or coerced into executing malicious tools on future invocations.
**Recommended Structural Change:** Implement strict input sanitization and semantic validation during transcript ingestion. Store verbatim data with cryptographic signatures of its source, and wrap recalled memories in strict XML/JSON schema bounds (`<untrusted_memory>`) when presenting them to the LLM.

## 4. Finding: Tight Coupling & Scaling Liability of Local File Locks
**Current Assumption:** RocksDB and SurrealKV can scale efficiently using exclusive local file locks for persistence, and dynamic fallback/retry loops (up to 10 attempts) are sufficient to handle lock contention.
**Attack Scenario (Load):** As the system scales to 10x concurrent agents or background compaction sweeps, file lock contention will exponentially increase, causing the retry loop to fail. This leads to dropped transactions, database corruption, or complete daemon lockups (as evidenced by current `cargo test` timeouts).
**Blast Radius:** Total database unavailability, dropped memories, and system deadlock across all active agents.
**Recommended Structural Change:** Decouple the database from the local file system using a dedicated remote/distributed database instance (e.g., a clustered SurrealDB instance or Postgres via gRPC) instead of local embedded KV engines.

## 5. Finding: Eval Framework Lacks Adversarial Honesty
**Current Assumption:** The `evals/swebench/eval.sh` framework (SWE-bench Verified) accurately measures the system's performance and reliability.
**Attack Scenario:** The system only optimizes for "happy path" code resolutions. It does not test how the model behaves when given conflicting instructions, hallucinated test files, or maliciously crafted PRs. The architecture is "honest" only in a sterile environment.
**Blast Radius:** The system appears robust on benchmarks but will silently fail or be easily compromised in real-world messy or adversarial codebases.
**Recommended Structural Change:** Augment the eval harness with an adversarial dataset (e.g., PromptMap, Garak, or custom perturbed SWE-bench issues with hidden prompt injections) to measure resilience and boundary enforcement, not just coding accuracy.
