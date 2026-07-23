## 2026-07-23 - Critical Shell Injection in Arbor HTR
**Vulnerability:** The Arbor HTR Parallel Verification Loop executes candidate changes and tests using raw POSIX shell invocation (`sh -c`) in git worktrees, creating a severe shell injection vulnerability that breaks intended agent isolation boundaries.
**Learning:** Shell evaluation with `sh -c` on dynamic, external (or LLM-generated) input provides an easy avenue for attackers or hallucinating models to run arbitrary commands on the host machine.
**Prevention:** Avoid `sh -c` and use explicit command and argument arrays (`Command::new("binary").arg(...)`) whenever possible. If shell capabilities are absolutely required, use strict input sanitization.

## 2026-07-23 - Data Race with Environment Variables
**Vulnerability:** Using `std::env::set_var` (an `unsafe` block in Rust 1.80+) in multi-threaded contexts (like `tokio` handlers in `mythrax-core/src/mcp_routes/vault_handlers.rs`) causes undefined behavior and data races.
**Learning:** Process-global environment variables cannot be safely mutated while other threads might be reading them (e.g. standard library functions, other crates).
**Prevention:** Never mutate the environment in a multi-threaded application; use thread-local state or pass explicit configuration objects down the call stack instead.

## 2026-07-23 - Cross-Session Prompt Injection via Unsanitized Hooks
**Vulnerability:** The pre-compaction hook in `mythrax-core/src/hooks/precompact.rs` extracts tool results and user inputs verbatim into episodic memory without sanitization.
**Learning:** Storing unsanitized inputs into long-term memory allows malicious payloads (prompt injections) to lay dormant and later compromise future sessions or context windows when recalled by the LLM.
**Prevention:** Always sanitize tool results and user inputs before appending them to memory or the active context window. Use robust parsing and escaping strategies.
