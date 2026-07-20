---
title: "Bug: Data race from unsafe std::env::set_var in async MCP vault_handler"
labels: ["bug", "agent-found"]
---

### Description
In `mythrax-core/src/mcp_routes/vault_handlers.rs`, the route handler temporarily mutates the global process environment variable `MYTHRAX_BOOTSTRAPPING` inside an `unsafe` block before yielding to an `await` point (`run_bootstrap_internal(...).await`), and then removes it. Because this runs inside a multi-threaded `tokio` asynchronous runtime, mutating process-wide environment variables creates a severe race condition. Any concurrently executing task (like another route handler, database compaction, or a background worker) reading or modifying the environment will experience undefined behavior, potentially reading a corrupted state or crashing the daemon.

### File and Line Number
* `mythrax-core/src/mcp_routes/vault_handlers.rs`, line 321 (and line 323 for `remove_var`)

### Minimal Reproducible Scenario
1. Send an HTTP request to the API gateway hitting the vault handler that triggers the non-async branch (e.g. `is_background = false`).
2. The handler executes `unsafe { std::env::set_var("MYTHRAX_BOOTSTRAPPING", "1"); }`.
3. Simultaneously, send another request that accesses an environment variable or triggers another handler doing the same.
4. The multi-threaded environment variable modification causes a data race, potentially causing a crash or incorrect logic execution for the second request.

### Severity
**High** - Thread-unsafe environment modification in async context leads to undefined behavior.

### Suggested Fix
Do not use `std::env::set_var` to pass temporary state to internal functions. Instead, refactor `run_bootstrap_internal` to accept a `bootstrap_mode: bool` argument or pass the configuration through the `state` context struct.

```rust
// Refactor run_bootstrap_internal signature:
let report_res = run_bootstrap_internal(state.clone(), dry_run, since, scope_str, force, /* is_bootstrap: */ true).await;
```
