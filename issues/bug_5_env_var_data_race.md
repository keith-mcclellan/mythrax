---
labels: bug, agent-found
---

# Thread safety data race using `std::env::set_var` in `vault_handlers.rs`

## Description
Modifying environment variables in a multi-threaded Rust context (like an `axum` handler running under `tokio`) poses a significant risk for data races and undefined behavior. The `MYTHRAX_BOOTSTRAPPING` environment variable is temporarily set and removed inside an unsafe block during the request lifecycle. If another concurrent thread attempts to read or write environment variables simultaneously, a data race can occur, potentially causing memory corruption or spurious failures.

## Location
- File: `mythrax-core/src/mcp_routes/vault_handlers.rs`
- Lines: 321, 323

## Minimal Reproducible Scenario
1. Start the API gateway and send a request that triggers `run_bootstrap_internal`.
2. Concurrently, send multiple other requests that spawn background tasks (which might implicitly or explicitly read environment variables, e.g., configuration parsing or spawning processes).
3. The `unsafe { std::env::set_var(...) }` runs while another thread reads from `std::env`.
4. This results in undefined behavior.

## Severity
Critical - Undefined Behavior, data race, potential crashes.

## Suggested Fix
Avoid using environment variables for local state control. Instead, pass `bootstrapping: bool` or a context struct down to `run_bootstrap_internal`, or use task-local variables (`tokio::task_local!`).