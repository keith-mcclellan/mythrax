# Specification: CTO Deep Architectural Refactor
## Functional Requirements

1. **Anti-Slop & Performance (No Panics, No $O(N)$ Hot-Paths)**
   - Fix $O(N)$ scan in `src/vault/watcher.rs:411` by utilizing a `BTreeMap<Instant, PathBuf>` or similar constant-time structure.
   - Refactor `api.rs`, `auth.rs`, `bench/metrics.rs`, `bench/runner.rs`, and ALL other module surfaces to propagate errors safely via `Result`. Remove all instances of `.unwrap()`, `.expect()`, `todo!()`, and `unimplemented!()`.
   - Adhere strictly to the *Anti-Slop & Quality Gate Directives* ensuring robust assertions in tests.

2. **Database Monolith Decomposition (Bite-Sized Modules)**
   - Decompose `src/db/backend.rs` (4030 lines), `src/db/crud_operations.rs` (3253 lines), and `src/db/search_pipeline.rs` (3135 lines) into smaller, hyper-focused modules (`connection.rs`, `migrations.rs`, and a rich `src/db/queries/` domain directory).
   - Ensure native async implementation natively across all downstream callsites.

3. **Cognitive Pipeline Refactor & Safety Guards**
   - Decompose `src/cognitive/pipeline.rs` into `orchestrator.rs` and a `signals/` directory.
   - Implement **Safe RAII Scope Guards** (using `Drop`) for all operational status flags and temporary database state cleanups.
   - Implement explicit `evict()` methods and write-on-evict safety for VRAM caches and LRU buffers.
   - Enforce strict lock ordering.

4. **Service Layer Extraction & Core App Decomposition**
   - Decompose `src/main.rs` and `src/api.rs`. Extract the daemon startup loop, HTTP routes, and background orchestration into a dedicated `src/daemon/` directory.
   - Refactor MCP Handlers into domain-specific service structs.
   - Create `src/mcp_routes/dtos.rs` to strictly isolate SurrealDB database types from the MCP transport layer.

5. **Documentation Standards**
   - Rewrite docstrings to comply with `docs:write-concisely`.
   - Document complex algorithms affirmatively.
