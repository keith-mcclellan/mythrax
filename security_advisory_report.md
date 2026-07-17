# Mythrax Security Advisory Report

## Critical Findings

### 1. SQL Injection Vulnerabilities via String Interpolation
**Description:** Multiple execution paths in the codebase construct SurrealDB queries using string interpolation (`format!`) with user-controlled or dynamically generated inputs, rather than using parameterized queries (`bind`). This allows arbitrary SQL execution if the input contains malicious queries.
**Locations:**
- `src/vault/ingestion.rs:47`: `let query_sql = format!("SELECT role, content FROM {};", table_name);`
- `src/mcp_routes/manage_handlers.rs:1480`: `let sql = format!("SELECT scope FROM {};", rec_id.table);`
- `src/cognitive/synthesis.rs:1218`: `let sql_wiki = format!("SELECT *, vector::similarity::cosine(embedding, $emb) AS similarity FROM wiki_node WHERE embedding <|200, {}|> $emb;", hnsw_ef);`
- `src/cognitive/synthesis.rs:1237`: `let sql_ep = format!("SELECT *, vector::similarity::cosine(embedding, $emb) AS similarity FROM episode WHERE node_type = 'procedural' AND embedding <|200, {}|> $emb;", hnsw_ef);`
- `src/mcp_routes/vault_handlers.rs:271-276`: `let delete_queries = format!("DELETE FROM chat_history WHERE session_id = '{}'; ...", sess);`
**Remediation:** Refactor all identified locations to use parameterized queries (`.bind()`) or `type::table()` for dynamic table names. Never use string interpolation for SQL queries.
**Estimated Effort:** Medium (1-2 days)

### 2. Hardcoded Fallback Auth Token
**Description:** The application uses a hardcoded fallback authentication token (`"secret-token"`) in tests and potentially production code if the token file is missing, allowing unauthorized access to API routes.
**Locations:**
- `src/api.rs`: Hardcoded `auth_token: "secret-token".to_string()` in test state and used in multiple test requests.
- git history: Commit analysis revealed a prior finding (HARD-001) regarding "secret-token" in `daemon.rs`, `main.rs`, and `db/backend.rs`.
**Remediation:** Remove the hardcoded fallback token. The application must generate a cryptographically random token and write it to `~/.mythrax/token` with `0600` permissions if one does not exist. Tests should mock the token generation or read a dynamically generated token.
**Estimated Effort:** Low (2-4 hours)

## High Findings

### 3. Unsafe Rust Blocks Modifying Environment Variables in Multithreaded Context
**Description:** The codebase uses `unsafe { std::env::set_var(...) }` and `unsafe { std::env::remove_var(...) }` in multithreaded asynchronous code (Tokio). Modifying environment variables is fundamentally thread-unsafe in Rust and can cause undefined behavior, data races, and crashes when other threads read the environment simultaneously.
**Locations:**
- `src/mcp_routes/vault_handlers.rs:325, 327`: Setting `MYTHRAX_BOOTSTRAPPING`.
- `src/cognitive/harvest.rs:255, 291, 300, 316`: Modifying `HOME` and `MYTHRAX_MOCK_LLM`.
- `src/bench/runner.rs:168, 886`: Setting `MYTHRAX_DAEMON_PORT`, `MYTHRAX_SESSION_ISOLATION`, `MYTHRAX_BENCH`.
- `src/store.rs:277, 281`: Setting `MYTHRAX_VAULT_ROOT`.
- `src/bin/inspect_failed_query.rs:41`: Setting `MYTHRAX_SESSION_ISOLATION`.
**Remediation:** Remove all `unsafe` blocks modifying environment variables. Use configuration structs, context objects, or thread-local storage to pass configuration state instead of relying on global environment variables. None of these blocks contain a documented safety justification.
**Estimated Effort:** Medium (2-3 days)

### 4. Vulnerable Dependencies in Cargo.lock (High Severity)
**Description:** Cargo audit revealed multiple dependencies with known high-severity vulnerabilities.
**Findings:**
- `lopdf v0.38.0`: Stack overflow via deeply nested PDF objects (RUSTSEC-2026-0187, Severity: 7.5 High). Solution: Upgrade to >=0.42.0.
- `quinn-proto v0.11.14`: Remote memory exhaustion from unbounded out-of-order stream reassembly (RUSTSEC-2026-0185, Severity: 7.5 High). Solution: Upgrade to >=0.11.15.
**Remediation:** Update the versions of the vulnerable dependencies in `Cargo.toml` and run `cargo update` to regenerate `Cargo.lock`.
**Estimated Effort:** Low (1-2 hours)

## Medium Findings

### 5. Unsafe FFI Calls to libc without Bounds Checking
**Description:** The daemon makes FFI calls to `libc::statfs` and `libc::kill` using `unsafe` blocks. While necessary for system interactions, these blocks lack documented safety comments explaining why the pointers are valid and the memory is safely initialized, violating Rust's `unsafe` guidelines.
**Locations:**
- `src/main.rs:277`: `unsafe { libc::kill(pid, 0) == 0 }`
- `src/daemon.rs:638, 639`: `let mut buf: libc::statfs = unsafe { std::mem::zeroed() }; let res = unsafe { libc::statfs(c_path.as_ptr(), &mut buf) };`
- `src/daemon.rs:656, 657`: Similar `statfs` calls.
**Remediation:** Add explicit `// SAFETY: ...` comments above every `unsafe` block detailing the invariants that uphold memory safety (e.g., explaining why `c_path.as_ptr()` outlives the call). Consider replacing raw `libc` calls with safer abstractions from crates like `nix` or `sysinfo` where possible.
**Estimated Effort:** Low (2-4 hours)

### 6. Vulnerable Dependencies in Cargo.lock (Medium Severity & Unmaintained)
**Description:** Cargo audit revealed dependencies with medium-severity vulnerabilities and unmaintained status.
**Findings:**
- `rsa v0.9.10`: Marvin Attack: potential key recovery through timing sidechannels (RUSTSEC-2023-0071, Severity: 5.9 Medium).
- `ammonia v4.1.2`: mXSS in ammonia via MathML `annotation-xml` encoding strip (RUSTSEC-2026-0193). Solution: Upgrade to >=4.1.3.
- `crossbeam-epoch v0.9.18`: Invalid pointer dereference in `fmt::Pointer` impl (RUSTSEC-2026-0204). Solution: Upgrade to >=0.9.20.
- Unmaintained crates: `atomic-polyfill`, `bincode`, `paste`, `proc-macro-error2`, `ttf-parser`.
**Remediation:** Update dependencies with available fixes. Evaluate migration paths for unmaintained crates.
**Estimated Effort:** Medium (2-3 days)

## Low Findings

### 7. Unsafe Trait Implementations for Mocks
**Description:** The codebase implements `Send` and `Sync` using `unsafe` for local embedders and LLM engines.
**Locations:**
- `src/llm/mod.rs:711, 712`: `unsafe impl Send for InProcessMlxEngine {}`
- `src/embeddings.rs:659, 661`: `unsafe impl Send for LocalEmbedder {}`
**Remediation:** The `InProcessMlxEngine` has a safety comment, but `LocalEmbedder` lacks one. Add a safety comment to `LocalEmbedder` explaining why it is safe to send and share across threads. Ensure no underlying non-thread-safe data (like raw pointers or un-synchronized `RefCell`s) is being shared.
**Estimated Effort:** Low (1 hour)
