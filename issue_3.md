# High: Unsafe Rust Blocks Modifying Environment Variables in Multithreaded Context

## Description
The codebase uses `unsafe { std::env::set_var(...) }` and `unsafe { std::env::remove_var(...) }` in multithreaded asynchronous code (Tokio). Modifying environment variables is fundamentally thread-unsafe in Rust and can cause undefined behavior, data races, and crashes when other threads read the environment simultaneously. Furthermore, none of these blocks contain a documented safety justification.

## Locations
- `src/mcp_routes/vault_handlers.rs:325, 327`: Setting `MYTHRAX_BOOTSTRAPPING`.
- `src/cognitive/harvest.rs:255, 291, 300, 316`: Modifying `HOME` and `MYTHRAX_MOCK_LLM`.
- `src/bench/runner.rs:168, 886`: Setting `MYTHRAX_DAEMON_PORT`, `MYTHRAX_SESSION_ISOLATION`, `MYTHRAX_BENCH`.
- `src/store.rs:277, 281`: Setting `MYTHRAX_VAULT_ROOT`.
- `src/bin/inspect_failed_query.rs:41`: Setting `MYTHRAX_SESSION_ISOLATION`.

## Remediation
Remove all `unsafe` blocks modifying environment variables. Use configuration structs, context objects, or thread-local storage to pass configuration state instead of relying on global environment variables.
