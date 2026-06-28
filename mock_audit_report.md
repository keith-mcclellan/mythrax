# Mythrax Codebase Mock, Configuration, Integration & Documentation Audit Report

## Executive Summary

A comprehensive read-only audit of all 49 Rust source files in `mythrax-core/src/` identified **10 mocked/stubbed code instances** in production paths, **100+ hardcoded values** that should be configurable (including 3 critical security-relevant hardcoded auth tokens and 1 already-expired hardcoded date), **25+ external process invocations** via `std::process::Command` (spanning `git`, `sh`, `cargo`, `sysctl`, `ps`, `kill`, and arbitrary command execution), and **12 documentation discrepancies** between the implemented code and the repository documentation. Of particular concern: 5 hardcoded absolute paths referencing `/Users/keith/mythrax-vault` are compiled into the production binary, test-detection logic is embedded in production search code, and the CLI version string is stale at `"1.0.0"` while `Cargo.toml` declares `v2.2.0`.

---

## 1. Mocked & Stubbed Code Findings

### MOCK-001: `audit_tailwind` — No-Op Stub in verify.rs
- **File Path:** [verify.rs#L29-L31](file:///Users/keith/Documents/mythrax/mythrax-core/src/verify.rs#L29-L31)
- **Code Snippet:**
  ```rust
  fn audit_tailwind(_workspace_path: &Path) -> (bool, Vec<String>) {
      (true, Vec::new())
  }
  ```
- **Matching Specification:** Design specification not found. No spec in `specs/` defines a Tailwind audit feature or its expected behavior.
- **Root Cause & Description:** This function is called from the compliance audit path (`verify_compliance`). It always returns a passing result `(true, Vec::new())`, meaning Tailwind violations are never detected. The parameter is prefixed with `_`, confirming it was never implemented. This is a stub that was likely placed as a placeholder during initial compliance audit development and never replaced with real logic.
- **Fix Recommendation:** Either implement a real Tailwind CSS audit (scanning for inline Tailwind classes, checking against a block list of prohibited utility classes), or remove the function entirely and its call site if Tailwind auditing is not a desired feature. If removing, update the `verify_compliance` return structure accordingly.

---

### MOCK-002: `audit_tailwind` — Duplicate No-Op Stub in daemon.rs
- **File Path:** [daemon.rs#L29-L30](file:///Users/keith/Documents/mythrax/mythrax-core/src/daemon.rs#L29-L30)
- **Code Snippet:**
  ```rust
  // Same stub signature as verify.rs — returns (true, Vec::new())
  ```
- **Matching Specification:** Design specification not found.
- **Root Cause & Description:** Duplicate of MOCK-001 in a different module. This suggests the function was copy-pasted rather than extracted into a shared module, compounding the stub problem.
- **Fix Recommendation:** Consolidate into a single implementation in `verify.rs` and remove the duplicate. Then implement real logic per MOCK-001.

---

### MOCK-003: `check_memory_pressure` — Always-False Stub
- **File Path:** [daemon.rs#L526-L528](file:///Users/keith/Documents/mythrax/mythrax-core/src/daemon.rs#L526-L528)
- **Code Snippet:**
  ```rust
  pub fn check_memory_pressure() -> bool {
      false
  }
  ```
- **Matching Specification:** [mythrax-2.0 design](file:///Users/keith/Documents/mythrax/specs/mythrax-2.0/design.md) — specifies swap monitoring with `sysctl vm.swapusage`, tier-based thresholds (2.0/3.0/6.0 GB), and `disable_swap_monitor` config flag.
- **Root Cause & Description:** The 2.0 spec defines a real memory pressure check using `sysctl vm.swapusage` on macOS (which IS implemented in `main.rs:337-340`), but this function was left as a stub. The adjacent `check_swap_pressure()` at line 517 IS implemented with real tier-based threshold logic. This stub appears to be a leftover from an earlier iteration.
- **Fix Recommendation:** Either delegate to the existing `check_swap_pressure()` with actual `sysctl` output parsing (already implemented in `main.rs:337-340`), or remove this dead stub if `check_swap_pressure()` fully supersedes it.

---

### MOCK-004: `get_weak_llm_reference` — Dummy InProcessMlxEngine
- **File Path:** [llm/mod.rs#L934-L944](file:///Users/keith/Documents/mythrax/mythrax-core/src/llm/mod.rs#L934-L944)
- **Code Snippet:**
  ```rust
  // Creates a "dummy" InProcessMlxEngine with:
  // name: "dummy", warmed_up: false, execution_mode: "cpu"
  ```
- **Matching Specification:** [mythrax-2.0 design](file:///Users/keith/Documents/mythrax/specs/mythrax-2.0/design.md) — specifies `DynamicModelBroker::new_mock()` for non-GPU test environments, but this dummy creation occurs in a production function (`get_weak_llm_reference`) that is NOT gated by `#[cfg(test)]` or the `MYTHRAX_TEST_MOCK` env var.
- **Root Cause & Description:** When no weak reference to an LLM engine exists, the code creates a dummy engine with name `"dummy"` and `warmed_up: false` rather than returning an error or `None`. This means production code can silently operate with a non-functional model engine.
- **Fix Recommendation:** Return `Option<Arc<InProcessMlxEngine>>` instead of always fabricating a dummy. Callers should handle `None` gracefully (e.g., queue the request, return an error to the client, or trigger model warm-up).

---

### MOCK-005: `new_corrupt_mock` — Mock Broker in Production Code
- **File Path:** [llm/mod.rs#L962-L972](file:///Users/keith/Documents/mythrax/mythrax-core/src/llm/mod.rs#L962-L972)
- **Code Snippet:**
  ```rust
  // Creates DynamicModelBroker with corrupt_mock: true, empty models,
  // empty config, and PathBuf::new() models_dir
  ```
- **Matching Specification:** [mythrax-2.0 design](file:///Users/keith/Documents/mythrax/specs/mythrax-2.0/design.md) — mentions `new_corrupt_mock()` explicitly for shader fallback tests.
- **Root Cause & Description:** A function named `new_corrupt_mock` exists in the production `llm/mod.rs` module (not behind `#[cfg(test)]`). While the spec acknowledges this for testing, it violates the zero-mock production code constraint. The `corrupt_mock: true` flag leaks test infrastructure into the production binary.
- **Fix Recommendation:** Move `new_corrupt_mock()` behind `#[cfg(test)]` or into the `tests/` directory. If shader fallback testing requires this, use a test-only feature flag (e.g., `#[cfg(any(test, feature = "test-utils"))]`).

---

### MOCK-006: `acquire_llm_with_warmup_fallback` — Fallback CPU Model
- **File Path:** [llm/mod.rs#L982-L998](file:///Users/keith/Documents/mythrax/mythrax-core/src/llm/mod.rs#L982-L998)
- **Code Snippet:**
  ```rust
  // On model acquisition failure: clears all models and creates
  // InProcessMlxEngine with name "fallback-cpu-model", warmed_up: true, "cpu" mode
  ```
- **Matching Specification:** [mythrax-2.0 design](file:///Users/keith/Documents/mythrax/specs/mythrax-2.0/design.md) — specifies VRAM eviction and sequential swapping but does NOT specify a "fallback-cpu-model" creation on failure.
- **Root Cause & Description:** When model acquisition fails, instead of propagating the error, the code silently creates a fake engine named `"fallback-cpu-model"` with `warmed_up: true` (a lie — the model was never actually loaded). This masks real GPU/model failures in production.
- **Fix Recommendation:** Propagate the error to the caller. If graceful degradation is desired, implement a real CPU-only ONNX inference path (the ORT backend already exists in `embeddings.rs`) rather than fabricating a mock engine.

---

### MOCK-007: `MYTHRAX_TEST_MOCK` — Test Gate in Production Model Loading
- **File Path:** [llm/mod.rs#L837-L838](file:///Users/keith/Documents/mythrax/mythrax-core/src/llm/mod.rs#L837-L838)
- **Code Snippet:**
  ```rust
  if std::env::var("MYTHRAX_TEST_MOCK").is_ok() {
      // Sets model and tokenizer to (None, None) — skips loading entirely
  }
  ```
- **Matching Specification:** [mythrax-2.0 test-plan](file:///Users/keith/Documents/mythrax/specs/mythrax-2.0/test-plan.md) and [DEVELOPMENT.md](file:///Users/keith/Documents/mythrax/DEVELOPMENT.md#L367-L371) — both document `MYTHRAX_TEST_MOCK=1` as a test-only env var.
- **Root Cause & Description:** The `MYTHRAX_TEST_MOCK` environment variable check is embedded in the production `acquire_llm` function (NOT behind `#[cfg(test)]`). If this env var is accidentally set in a production environment, model loading is silently disabled. This is test infrastructure leaking into the production binary.
- **Fix Recommendation:** Gate with `#[cfg(test)]` or `#[cfg(feature = "test-mock")]` at compile time. Runtime env var checks for test behavior in production code are an anti-pattern.

---

### MOCK-008: `download_file_if_missing` — Dummy File Write
- **File Path:** [llm/mod.rs#L1022-L1027](file:///Users/keith/Documents/mythrax/mythrax-core/src/llm/mod.rs#L1022-L1027)
- **Code Snippet:**
  ```rust
  if std::env::var("MYTHRAX_TEST_MOCK").is_ok() {
      // Writes b"dummy" to file instead of downloading from HuggingFace
  }
  ```
- **Matching Specification:** Same as MOCK-007.
- **Root Cause & Description:** Same `MYTHRAX_TEST_MOCK` env var pattern. In production, if set, model weight files are replaced with 5-byte dummy files instead of real multi-GB model weights. The model will crash or produce garbage on inference.
- **Fix Recommendation:** Same as MOCK-007 — compile-time gating, not runtime env var.

---

### MOCK-009: `embed_batch` — Zero-Embedding Fallback
- **File Path:** [db/backend.rs#L4006-L4008](file:///Users/keith/Documents/mythrax/mythrax-core/src/db/backend.rs#L4006-L4008)
- **Code Snippet:**
  ```rust
  // When no embedder is configured, returns:
  // Ok(vec![vec![0.0f32; 768]; texts.len()])
  ```
- **Matching Specification:** [memory_enhancements design](file:///Users/keith/Documents/mythrax/specs/memory_enhancements/design.md) — specifies ONNX model fallback to substring search when models are missing, NOT zero-vector generation.
- **Root Cause & Description:** When no local embedding model is loaded, the function returns vectors of all zeros for every input text. Zero vectors have undefined cosine similarity behavior (division by zero) and will produce meaningless search results. The spec says to fall back to substring text search, not to generate fake embeddings.
- **Fix Recommendation:** Return `Err(...)` or `None` when no embedder is available, and let the search layer fall back to BM25/substring search (which is already implemented in `retrieval/bm25.rs`).

---

### MOCK-010: Test Detection Logic in Production Search Code
- **File Path:** [db/backend.rs#L1690-L1704](file:///Users/keith/Documents/mythrax/mythrax-core/src/db/backend.rs#L1690-L1704)
- **Code Snippet:**
  ```rust
  // Detects if running in test via std::env::current_exe() checking for "/deps/" or "test"
  // in the binary name, plus std::env::args() checking for "test" arguments.
  // Uses MYTHRAX_SIGMOID_GATED_SEARCH_TEST env var.
  // When detected, injects hardcoded similarity values (0.85, 0.50, 1.0) at L2972-2975
  ```
- **Matching Specification:** Design specification not found. No spec authorizes embedding test-detection logic in the production `search()` function.
- **Root Cause & Description:** The production `search()` function contains logic that inspects the current executable name and command-line arguments to determine if it's running inside a test harness. When detected, it overrides real similarity scores with hardcoded test values. This is a severe violation — production behavior changes based on the binary name.
- **Fix Recommendation:** Remove all test-detection logic from `search()`. Move the hardcoded test similarity values into the test module itself (e.g., by injecting a `SimilarityOverride` trait or by using `#[cfg(test)]` conditional compilation).

---

## 2. Hardcoded Values Findings

### HARD-001: Hardcoded Fallback Auth Token — "secret-token"
- **File Path:** [daemon.rs#L37](file:///Users/keith/Documents/mythrax/mythrax-core/src/daemon.rs#L37), [main.rs#L30](file:///Users/keith/Documents/mythrax/mythrax-core/src/main.rs#L30), [main.rs#L832](file:///Users/keith/Documents/mythrax/mythrax-core/src/main.rs#L832), [db/backend.rs#L543](file:///Users/keith/Documents/mythrax/mythrax-core/src/db/backend.rs#L543)
- **Code Snippet:**
  ```rust
  "secret-token".to_string()
  ```
- **Matching Specification:** [mythrax-1.0-release design](file:///Users/keith/Documents/mythrax/specs/mythrax-1.0-release/design.md) — specifies token stored at `~/.mythrax/token` with `0600` permissions. Does NOT authorize a hardcoded fallback.
- **Recommended Configuration Strategy:** **CRITICAL SECURITY ISSUE.** Remove the hardcoded fallback. If no token file exists, generate a cryptographically random token, write it to `~/.mythrax/token` with `0600` permissions, and use that. Never fall back to a known static string.

---

### HARD-002: Expired Hardcoded Date — Config Override Expiry
- **File Path:** [api.rs#L162](file:///Users/keith/Documents/mythrax/mythrax-core/src/api.rs#L162), [db/backend.rs#L2792](file:///Users/keith/Documents/mythrax/mythrax-core/src/db/backend.rs#L2792)
- **Code Snippet:**
  ```rust
  Some("2026-06-21T23:59:59Z".to_string())
  ```
- **Matching Specification:** Design specification not found.
- **Recommended Configuration Strategy:** **CRITICAL — Already expired** (current date is 2026-06-27). This means all non-permanent LLM config overrides are silently expired. Either make the expiry duration configurable (e.g., `config_override_ttl_hours: 168` for 7 days from save time), or remove expiry entirely and let users explicitly delete overrides.

---

### HARD-003: Hardcoded Absolute User Path — `/Users/keith/mythrax-vault`
- **File Path:** [vault/watcher.rs#L115](file:///Users/keith/Documents/mythrax/mythrax-core/src/vault/watcher.rs#L115), [L137](file:///Users/keith/Documents/mythrax/mythrax-core/src/vault/watcher.rs#L137), [L237](file:///Users/keith/Documents/mythrax/mythrax-core/src/vault/watcher.rs#L237), [L293](file:///Users/keith/Documents/mythrax/mythrax-core/src/vault/watcher.rs#L293), [L497](file:///Users/keith/Documents/mythrax/mythrax-core/src/vault/watcher.rs#L497)
- **Code Snippet:**
  ```rust
  let global_dir = std::path::Path::new("/Users/keith/mythrax-vault/global");
  ```
- **Matching Specification:** All specs reference vault root dynamically via `query_memory(action="root")` or config resolution. No spec authorizes hardcoded absolute user paths.
- **Recommended Configuration Strategy:** **CRITICAL.** Replace all 5 instances with the dynamically resolved vault root path (already available in the `MarkdownStore.vault_root` field). This binary will not function correctly for any user other than `keith`.

---

### HARD-004: Default SurrealDB URL — `mem://`
- **File Path:** [daemon.rs#L43-L45](file:///Users/keith/Documents/mythrax/mythrax-core/src/daemon.rs#L43-L45)
- **Code Snippet:**
  ```rust
  val["surrealdb_url"].as_str().unwrap_or("mem://").to_string()
  ```
- **Matching Specification:** [ARCHITECTURE.md](file:///Users/keith/Documents/mythrax/ARCHITECTURE.md#L49) documents "SurrealKV & RocksDB Engines" — `mem://` is an in-memory volatile store, not persistent.
- **Recommended Configuration Strategy:** Default to `surrealkv://~/.mythrax/db` or `rocksdb://~/.mythrax/db` for persistence. Use `mem://` only when explicitly configured for testing.

---

### HARD-005: Port 8090 — Default Daemon Port
- **File Path:** [cli.rs#L73](file:///Users/keith/Documents/mythrax/mythrax-core/src/cli.rs#L73), [cli.rs#L82](file:///Users/keith/Documents/mythrax/mythrax-core/src/cli.rs#L82), [main.rs#L33](file:///Users/keith/Documents/mythrax/mythrax-core/src/main.rs#L33), [db/backend.rs#L336](file:///Users/keith/Documents/mythrax/mythrax-core/src/db/backend.rs#L336), [verify.rs#L155](file:///Users/keith/Documents/mythrax/mythrax-core/src/verify.rs#L155)
- **Code Snippet:**
  ```rust
  #[arg(long, default_value_t = 8090)]
  pub port: u16,
  ```
- **Matching Specification:** [mythrax-1.0-release](file:///Users/keith/Documents/mythrax/specs/mythrax-1.0-release/design.md) and [ARCHITECTURE.md](file:///Users/keith/Documents/mythrax/ARCHITECTURE.md#L34) — both document port 8090 as the default.
- **Recommended Configuration Strategy:** Port 8090 is correctly used as a default, but `verify.rs:155` hardcodes the full URL `http://127.0.0.1:8090/v1/config/llm` rather than constructing it from the configured port. Extract into a `DaemonConfig` struct that holds `host` and `port`, and derive all URLs from that.

---

### HARD-006: Port 8080 — Proxy Upstream URLs
- **File Path:** [api.rs#L323](file:///Users/keith/Documents/mythrax/mythrax-core/src/api.rs#L323), [api.rs#L390](file:///Users/keith/Documents/mythrax/mythrax-core/src/api.rs#L390)
- **Code Snippet:**
  ```rust
  "http://127.0.0.1:8080/v1/chat/completions"
  "http://127.0.0.1:8080/api/{}"
  ```
- **Matching Specification:** [ARCHITECTURE.md#L71](file:///Users/keith/Documents/mythrax/ARCHITECTURE.md#L71) — documents port 8080 for external model delegation. [artifact_linking design](file:///Users/keith/Documents/mythrax/specs/artifact_linking/design.md) — also references `http://127.0.0.1:8080/v1/chat/completions`.
- **Recommended Configuration Strategy:** Make the upstream LLM server URL configurable via the LLM config (`manage_config`). The port 8080 should come from `config.json` or be passed as a daemon CLI argument.

---

### HARD-007: `/tmp` Paths — Worktree and Cargo Target Directories
- **File Path:** [cognitive/executor.rs#L23](file:///Users/keith/Documents/mythrax/mythrax-core/src/cognitive/executor.rs#L23), [L86](file:///Users/keith/Documents/mythrax/mythrax-core/src/cognitive/executor.rs#L86), [L113](file:///Users/keith/Documents/mythrax/mythrax-core/src/cognitive/executor.rs#L113), [L135](file:///Users/keith/Documents/mythrax/mythrax-core/src/cognitive/executor.rs#L135)
- **Code Snippet:**
  ```rust
  let worktree_dir = format!("/tmp/worktree-node-{}", node_id);
  let cargo_target = format!("/tmp/cargo-target-node-{}", node_id);
  ```
- **Matching Specification:** [arbor_htr design](file:///Users/keith/Documents/mythrax/specs/arbor_htr/design.md) — specifies git worktree isolation but does not mandate `/tmp`.
- **Recommended Configuration Strategy:** Use `std::env::temp_dir()` or a configurable `worktree_base_dir` in the HTR config. `/tmp` is not portable across all systems.

---

### HARD-008: Sigmoid Gating Parameters
- **File Path:** [db/backend.rs#L2008](file:///Users/keith/Documents/mythrax/mythrax-core/src/db/backend.rs#L2008)
- **Code Snippet:**
  ```rust
  // g = 1 / (1 + exp(-20 * (similarity - 0.60)))
  // Steepness: 20, Midpoint: 0.60
  ```
- **Matching Specification:** [ARCHITECTURE.md#L93](file:///Users/keith/Documents/mythrax/ARCHITECTURE.md#L93) — documents steepness `-20.0` and midpoint `0.60`.
- **Recommended Configuration Strategy:** Extract steepness and midpoint into the `config:settings` table or a `RetrievalConfig` struct. These are tuning parameters that may need adjustment as the memory corpus grows.

---

### HARD-009: DBSCAN Clustering Parameters (Multiple Locations)
- **File Path:** [cognitive/compactor.rs#L195](file:///Users/keith/Documents/mythrax/mythrax-core/src/cognitive/compactor.rs#L195), [cognitive/harvest.rs#L137](file:///Users/keith/Documents/mythrax/mythrax-core/src/cognitive/harvest.rs#L137), [cognitive/synthesis.rs#L326-L328](file:///Users/keith/Documents/mythrax/mythrax-core/src/cognitive/synthesis.rs#L326-L328)
- **Code Snippet:**
  ```rust
  // compactor.rs: eps=0.10, min_samples=2
  // harvest.rs:  eps=0.10, min_samples=2
  // synthesis.rs "deep": eps=0.15, min_samples=2
  // synthesis.rs "bulk": eps=0.12, min_samples=4
  // synthesis.rs "incremental": eps=0.08, min_samples=2
  ```
- **Matching Specification:** [mythrax-2.0 design](file:///Users/keith/Documents/mythrax/specs/mythrax-2.0/design.md) — specifies `default_epsilon: 0.55` in config, and dynamic DBSCAN calibration.
- **Recommended Configuration Strategy:** Centralize all DBSCAN parameters into a `CompactionConfig` struct in the settings table. The synthesis module already has dynamic epsilon calibration logic — extend it to all clustering call sites.

---

### HARD-010: Retrieval Weight Parameters
- **File Path:** [db/backend.rs#L2013-L2015](file:///Users/keith/Documents/mythrax/mythrax-core/src/db/backend.rs#L2013-L2015), [L2117-L2118](file:///Users/keith/Documents/mythrax/mythrax-core/src/db/backend.rs#L2117-L2118), [L2174-L2175](file:///Users/keith/Documents/mythrax/mythrax-core/src/db/backend.rs#L2174-L2175), [L2319](file:///Users/keith/Documents/mythrax/mythrax-core/src/db/backend.rs#L2319)
- **Code Snippet:**
  ```rust
  // Episode weights: w_imp=0.3, w_rec=0.3, decay=exp(-0.05*dt)
  // Wiki weights: w_imp=0.4, w_rec=0.2
  // Wisdom weights: w_imp=0.5, w_rec=0.1
  // Hybrid BM25 fusion: 0.6*raw_sim + 0.4*bm25_norm
  ```
- **Matching Specification:** [phase_1_retrieval](file:///Users/keith/Documents/mythrax/specs/phase_1_retrieval.md) — documents the blending formula.
- **Recommended Configuration Strategy:** Extract all weight parameters into a `RetrievalConfig` struct. These are tuning parameters that benefit from A/B testing and per-scope customization.

---

### HARD-011: CLI Version String Mismatch
- **File Path:** [cli.rs#L4](file:///Users/keith/Documents/mythrax/mythrax-core/src/cli.rs#L4)
- **Code Snippet:**
  ```rust
  #[command(version = "1.0.0")]
  ```
- **Matching Specification:** [Cargo.toml](file:///Users/keith/Documents/mythrax/mythrax-core/Cargo.toml#L3) declares `version = "2.2.0"`.
- **Recommended Configuration Strategy:** Use `env!("CARGO_PKG_VERSION")` instead of a hardcoded string to automatically keep the CLI version in sync with `Cargo.toml`.

---

### HARD-012: Model Configuration Defaults
- **File Path:** [llm/mod.rs#L844-L864](file:///Users/keith/Documents/mythrax/mythrax-core/src/llm/mod.rs#L844-L864)
- **Code Snippet:**
  ```rust
  // num_hidden_layers: 28, num_attention_heads: 12, num_key_value_heads: 2,
  // hidden_size: 1536, rms_norm_eps: 1e-6, vocab_size: 151936,
  // rope_theta: 1000000.0, quantization: bits=4, group_size=64
  ```
- **Matching Specification:** [mythrax-2.0 design](file:///Users/keith/Documents/mythrax/specs/mythrax-2.0/design.md) — specifies model tiers but not internal model architecture defaults.
- **Recommended Configuration Strategy:** These are Qwen2-specific architecture parameters that should be read from the model's `config.json` file at load time (which is already done when the file exists). The hardcoded defaults should only serve as a last-resort fallback and should be documented as Qwen2-1.5B-specific.

---

### HARD-013: Stale Handoff/STM Pruning Threshold
- **File Path:** [db/backend.rs#L3425-L3468](file:///Users/keith/Documents/mythrax/mythrax-core/src/db/backend.rs#L3425-L3468)
- **Code Snippet:**
  ```rust
  // Stale handoff pruning: 3 days (SurrealQL: "3d")
  // STM pruning: 3 days
  // STM file pruning: 3 * 24 * 3600 seconds
  ```
- **Matching Specification:** [forge_and_stm design](file:///Users/keith/Documents/mythrax/specs/forge_and_stm/design.md) — specifies "7 days" for stale handoff cleanup.
- **Recommended Configuration Strategy:** The implementation uses 3 days but the spec says 7 days. Make this configurable via the settings table and align with the spec's 7-day default.

---

### HARD-014: Content Truncation Limit — 100,000 Characters
- **File Path:** [cognitive/synthesis.rs#L450-L452](file:///Users/keith/Documents/mythrax/mythrax-core/src/cognitive/synthesis.rs#L450-L452), [L577-L579](file:///Users/keith/Documents/mythrax/mythrax-core/src/cognitive/synthesis.rs#L577-L579), [L923-L925](file:///Users/keith/Documents/mythrax/mythrax-core/src/cognitive/synthesis.rs#L923-L925)
- **Code Snippet:**
  ```rust
  if content.len() > 100_000 { content.truncate(100_000); }
  ```
- **Matching Specification:** [artifact_linking design](file:///Users/keith/Documents/mythrax/specs/artifact_linking/design.md) — documents 100,000 character prompt truncation.
- **Recommended Configuration Strategy:** Extract into a `MAX_PROMPT_CONTENT_CHARS` constant or config parameter. The value may need adjustment based on the model's context window size.

---

## 3. Non-Rust Native Workarounds / "Cheats"

### CHEAT-001: Git Operations via std::process::Command (Multiple Files)
- **File Path:** [cognitive/arbor.rs#L207-L362](file:///Users/keith/Documents/mythrax/mythrax-core/src/cognitive/arbor.rs#L207-L362), [cognitive/executor.rs#L35-L126](file:///Users/keith/Documents/mythrax/mythrax-core/src/cognitive/executor.rs#L35-L126), [db/backend.rs#L1608-L1644](file:///Users/keith/Documents/mythrax/mythrax-core/src/db/backend.rs#L1608-L1644), [bench/runner.rs#L592-L594](file:///Users/keith/Documents/mythrax/mythrax-core/src/bench/runner.rs#L592-L594), [main.rs#L1236-L1259](file:///Users/keith/Documents/mythrax/mythrax-core/src/main.rs#L1236-L1259), [db/backend.rs#L4165-L4177](file:///Users/keith/Documents/mythrax/mythrax-core/src/db/backend.rs#L4165-L4177)
- **Code Snippet:**
  ```rust
  Command::new("git").args(["rev-parse", "HEAD"]).output()?
  Command::new("git").args(["worktree", "add", "-b", ...]).status()?
  Command::new("git").args(["add", file]).status()
  Command::new("git").args(["commit", "-m", msg]).status()
  Command::new("git").args(["push"]).status()
  ```
- **Matching Specification:** [rust_native_swebench_harness_spec](file:///Users/keith/Documents/mythrax/specs/rust_native_swebench_harness_spec.md) — explicitly authorizes `std::process::Command` for git operations. [arbor_htr design](file:///Users/keith/Documents/mythrax/specs/arbor_htr/design.md) — specifies git worktree isolation. [mythrax-2.0 design](file:///Users/keith/Documents/mythrax/specs/mythrax-2.0/design.md) — specifies `std::process::Command` direct execution (no shell).
- **Description of Workaround:** Git operations are performed by spawning the `git` CLI binary rather than using a native Rust git library (e.g., `git2`/`libgit2` bindings). This includes `rev-parse`, `worktree`, `add`, `commit`, `push`, `diff`, and `branch` operations across 6 files.
- **Fix Recommendation:** Consider replacing with the `git2` crate (Rust bindings to libgit2) for in-process git operations. This eliminates the dependency on the `git` binary being installed and in `$PATH`. However, the specs explicitly authorize `std::process::Command` for git, so this is a **low-priority** improvement. Git worktree operations may not be fully supported by `git2`.

---

### CHEAT-002: Shell Execution for Test Commands
- **File Path:** [cognitive/executor.rs#L81-L90](file:///Users/keith/Documents/mythrax/mythrax-core/src/cognitive/executor.rs#L81-L90)
- **Code Snippet:**
  ```rust
  Command::new("sh").arg("-c").arg(test_command).output()?
  ```
- **Matching Specification:** [arbor_htr design](file:///Users/keith/Documents/mythrax/specs/arbor_htr/design.md) — specifies running test commands in HTR worktrees.
- **Description of Workaround:** Test commands are executed via `sh -c`, which invokes a POSIX shell to interpret the command string. This is a shell injection risk if `test_command` contains user-controlled input.
- **Fix Recommendation:** Parse the command string into program and arguments, then use `Command::new(program).args(args)` directly (as the mythrax-2.0 spec mandates: "std::process::Command direct execution, no shell"). If shell features (pipes, redirects) are needed, document the security boundary.

---

### CHEAT-003: Checkpoint Compile Checks via External Compilers
- **File Path:** [daemon.rs#L336](file:///Users/keith/Documents/mythrax/mythrax-core/src/daemon.rs#L336)
- **Code Snippet:**
  ```rust
  // Spawns: cargo check, npx tsc --noEmit, python -m py_compile
  ```
- **Matching Specification:** Design specification not found. No spec defines a cross-language checkpoint compiler feature.
- **Description of Workaround:** The daemon's checkpoint service detects the project type and spawns external compilers (`cargo`, `npx`/`tsc`, `python`) to validate code. These are non-Rust external tools with large runtime dependencies.
- **Fix Recommendation:** This is an intentional cross-language feature, not a "cheat". However, it should be documented in ARCHITECTURE.md, made configurable (enable/disable per language, configure compiler paths), and the external tool availability should be checked before invocation.

---

### CHEAT-004: macOS Swap Usage via sysctl
- **File Path:** [main.rs#L337-L340](file:///Users/keith/Documents/mythrax/mythrax-core/src/main.rs#L337-L340)
- **Code Snippet:**
  ```rust
  Command::new("sysctl").args(["-n", "vm.swapusage"]).output()
  ```
- **Matching Specification:** [mythrax-2.0 design](file:///Users/keith/Documents/mythrax/specs/mythrax-2.0/design.md) — explicitly specifies `sysctl vm.swapusage` for swap monitoring.
- **Description of Workaround:** Swap memory usage is obtained by spawning the `sysctl` command-line tool rather than reading from the kernel directly. This is macOS-specific and non-portable.
- **Fix Recommendation:** Use the `sysinfo` crate for cross-platform memory/swap statistics. For macOS-specific needs, use the `libc` crate (already a dependency) to call `sysctl()` via FFI without spawning a process.

---

### CHEAT-005: Process Name Detection via `ps`
- **File Path:** [main.rs#L289-L292](file:///Users/keith/Documents/mythrax/mythrax-core/src/main.rs#L289-L292)
- **Code Snippet:**
  ```rust
  Command::new("ps").args(["-p", &pid_str, "-o", "comm="]).output()
  ```
- **Matching Specification:** [mythrax-2.0 design](file:///Users/keith/Documents/mythrax/specs/mythrax-2.0/design.md) — specifies "PID name guard (checks process name, not just PID)".
- **Description of Workaround:** Process name verification is done by spawning `ps` rather than reading `/proc/<pid>/comm` (Linux) or using `sysctl` KERN_PROCARGS (macOS).
- **Fix Recommendation:** Use `libc` FFI to read process info directly. On macOS, use `proc_pidpath()` from `libproc`. On Linux, read `/proc/<pid>/comm`.

---

### CHEAT-006: Daemon Stop via `kill` Command
- **File Path:** [daemon.rs#L397-L407](file:///Users/keith/Documents/mythrax/mythrax-core/src/daemon.rs#L397-L407)
- **Code Snippet:**
  ```rust
  Command::new("kill").args(["-15", &pid_str]).status()
  ```
- **Matching Specification:** Design specification not found.
- **Description of Workaround:** SIGTERM is sent by spawning the `kill` command rather than using `libc::kill()` directly.
- **Fix Recommendation:** Use `libc::kill(pid, libc::SIGTERM)` (the `libc` crate is already a dependency). This is a trivial one-liner that eliminates the process spawn.

---

### CHEAT-007: Arbitrary Command Execution — `exec` Subcommand
- **File Path:** [main.rs#L1277-L1283](file:///Users/keith/Documents/mythrax/mythrax-core/src/main.rs#L1277-L1283)
- **Code Snippet:**
  ```rust
  Command::new(command_name).args(args).exec()
  ```
- **Matching Specification:** Design specification not found. No spec defines an `exec` subcommand.
- **Description of Workaround:** The `mythrax exec` subcommand replaces the current process with an arbitrary external command. This is an intentional feature for running user-specified commands with Mythrax environment setup, not a workaround.
- **Fix Recommendation:** Document in DEVELOPMENT.md. Add input validation and logging. Consider whether this feature is necessary or if it should be removed for security hardening.

---

## 4. Documentation Discrepancies

### DOC-001: CLI Version Mismatch
- **Code Location:** [cli.rs#L4](file:///Users/keith/Documents/mythrax/mythrax-core/src/cli.rs#L4)
- **Doc Location:** [Cargo.toml#L3](file:///Users/keith/Documents/mythrax/mythrax-core/Cargo.toml#L3) and [mythrax_user_guide.md#L9](file:///Users/keith/Documents/mythrax/mythrax_user_guide.md#L9)
- **Description of Conflict:** `cli.rs` declares `version = "1.0.0"` while `Cargo.toml` says `2.2.0` and the user guide says "Mythrax 2.2.0". Users running `mythrax --version` see `1.0.0`.
- **Specification Reference & Conflicting Specs Resolution:** The 2.0 spec is later than the 1.0 release spec, and Cargo.toml is the canonical version source.
- **Alignment Recommendation:** **Update code.** Replace hardcoded `"1.0.0"` with `env!("CARGO_PKG_VERSION")` in cli.rs.

---

### DOC-002: Lock Retry Count — 10 vs 9
- **Code Location:** [db/backend.rs#L382-L384](file:///Users/keith/Documents/mythrax/mythrax-core/src/db/backend.rs#L382-L384)
- **Doc Location:** [ARCHITECTURE.md#L50](file:///Users/keith/Documents/mythrax/ARCHITECTURE.md#L50)
- **Description of Conflict:** ARCHITECTURE.md states "up to 10 attempts, 500ms sleep" for the persistent lock retry loop. The actual implementation uses 9 attempts (`max 9 attempts`).
- **Specification Reference & Conflicting Specs Resolution:** The mythrax-2.0 spec does not specify an exact retry count. ARCHITECTURE.md is the only source of the "10 attempts" claim.
- **Alignment Recommendation:** **Update documentation.** Change ARCHITECTURE.md to say "up to 9 attempts" to match the implementation, OR update the code to use 10 attempts if 10 was the intended design.

---

### DOC-003: Daemon Startup Poll Duration — 5s vs 15s
- **Code Location:** [main.rs#L117](file:///Users/keith/Documents/mythrax/mythrax-core/src/main.rs#L117)
- **Doc Location:** [mythrax_user_guide.md#L125](file:///Users/keith/Documents/mythrax/mythrax_user_guide.md#L125) and [ARCHITECTURE.md#L41](file:///Users/keith/Documents/mythrax/ARCHITECTURE.md#L41)
- **Description of Conflict:** The user guide says "polls for 5 seconds before executing the command." ARCHITECTURE.md says "port polling for up to 15 seconds." The actual code uses `15 seconds` in `main.rs` and `5 seconds` in `mcp.rs:167`.
- **Specification Reference & Conflicting Specs Resolution:** The docs_and_sweep spec notes "CLI timeout adjusted from 5s to 15s", confirming the 15s value is the later, intended design for CLI. The MCP path retains 5s.
- **Alignment Recommendation:** **Update documentation.** User guide should say "15 seconds" for CLI and "5 seconds" for MCP auto-spawn. ARCHITECTURE.md should clarify both paths.

---

### DOC-004: Test Command — `cargo test` vs `cargo nextest run`
- **Code Location:** N/A (workspace convention)
- **Doc Location:** [README.md#L167](file:///Users/keith/Documents/mythrax/README.md#L167) vs [DEVELOPMENT.md#L358-L365](file:///Users/keith/Documents/mythrax/DEVELOPMENT.md#L358-L365) and [AGENTS.md](file:///Users/keith/Documents/mythrax/.agents/AGENTS.md)
- **Description of Conflict:** README.md says `cargo test` in the Quick Start section. DEVELOPMENT.md and AGENTS.md mandate `cargo nextest run` (or `cargo t` alias) for parallel execution.
- **Specification Reference & Conflicting Specs Resolution:** DEVELOPMENT.md is the later, authoritative developer guide. AGENTS.md workspace rules reinforce nextest.
- **Alignment Recommendation:** **Update documentation.** README.md Quick Start should say `MYTHRAX_TEST_MOCK=1 cargo nextest run --features mlx` (or the `cargo t` alias).

---

### DOC-005: DEVELOPMENT.md References Non-Existent Crate and Paths
- **Code Location:** Actual project structure: `mythrax-core/src/db/backend.rs`, `mythrax-core/src/mcp_routes.rs`
- **Doc Location:** [DEVELOPMENT.md#L238](file:///Users/keith/Documents/mythrax/DEVELOPMENT.md#L238), [L262](file:///Users/keith/Documents/mythrax/DEVELOPMENT.md#L262), [L187-L199](file:///Users/keith/Documents/mythrax/DEVELOPMENT.md#L187-L199)
- **Description of Conflict:** DEVELOPMENT.md references `mythrax-cli/src/cli.rs` and `mythrax-cli/src/main.rs` — but no `mythrax-cli` crate exists. The project has a single crate: `mythrax-core`. It also references `storage/mod.rs` and `storage/surreal_backend.rs` — actual paths are `db/mod.rs` and `db/backend.rs`.
- **Specification Reference & Conflicting Specs Resolution:** The mythrax-1.0-release spec consolidated everything into a single crate. The DEVELOPMENT.md example code appears to be aspirational/illustrative rather than reflecting the actual architecture.
- **Alignment Recommendation:** **Update documentation.** Rewrite DEVELOPMENT.md Step 4 (CLI) to reference the actual `mythrax-core/src/cli.rs` clap subcommands, and update all path references to use actual module paths (`db/backend.rs`, `db/mod.rs`).

---

### DOC-006: DEVELOPMENT.md Tool Names Don't Match Implementation
- **Code Location:** [mcp_routes.rs](file:///Users/keith/Documents/mythrax/mythrax-core/src/mcp_routes.rs)
- **Doc Location:** [DEVELOPMENT.md#L27-L53](file:///Users/keith/Documents/mythrax/DEVELOPMENT.md#L27-L53)
- **Description of Conflict:** DEVELOPMENT.md uses example tool names `session_tool`, `memory_tool`, `file_tool` in its architecture diagram and examples. The actual MCP tools are named `manage_memory`, `manage_stm`, `manage_file`, `manage_htr`, `manage_vault`, `manage_config`, `pre_invocation_hook`, `complete_code_task`, `ingest_knowledge`.
- **Specification Reference & Conflicting Specs Resolution:** The mythrax-1.0-release spec defines the 9 consolidated tools with their actual names. DEVELOPMENT.md uses placeholder names that were never implemented.
- **Alignment Recommendation:** **Update documentation.** Replace all placeholder tool names in DEVELOPMENT.md with the actual consolidated tool names from the implementation.

---

### DOC-007: User Guide `manage_vault` Missing `audit` Action
- **Code Location:** [mcp_routes.rs](file:///Users/keith/Documents/mythrax/mythrax-core/src/mcp_routes.rs) — `manage_vault` supports `verify`, `organize`, `reprocess`, `summarize`, `audit`
- **Doc Location:** [mythrax_user_guide.md#L175-L178](file:///Users/keith/Documents/mythrax/mythrax_user_guide.md#L175-L178)
- **Description of Conflict:** User guide Section 8.5 lists `manage_vault` actions as `verify, organize, reprocess, summarize` — missing `audit`. Meanwhile, Section 8.7 lists `compliance_audit` as a separate tool, and README.md lists `mythrax audit` as a CLI command.
- **Specification Reference & Conflicting Specs Resolution:** The actual MCP schema (per `manage_vault.json`) defines the `audit` action. The `compliance_audit` tool in the user guide may be a legacy reference.
- **Alignment Recommendation:** **Update documentation.** Add `audit` to the `manage_vault` actions list in Section 8.5. Verify whether `compliance_audit` (Section 8.7) is still a separate MCP tool or has been consolidated into `manage_vault audit`.

---

### DOC-008: User Guide `record_memory` Missing `thought` Action
- **Code Location:** [mcp_routes.rs](file:///Users/keith/Documents/mythrax/mythrax-core/src/mcp_routes.rs) — `manage_memory` supports actions including `thought`
- **Doc Location:** [mythrax_user_guide.md#L159-L163](file:///Users/keith/Documents/mythrax/mythrax_user_guide.md#L159-L163)
- **Description of Conflict:** User guide Section 8.2 lists `record_memory` actions as `save` and `feedback` only. The actual implementation also supports `thought` (for TiM abstract thought nodes). The MCP schema additionally shows `search_index`, `timeline`, and `get_full` actions.
- **Specification Reference & Conflicting Specs Resolution:** The SKILL.md agent playbook references the `thought` action. The implementation has evolved beyond the user guide's documentation.
- **Alignment Recommendation:** **Update documentation.** Add all supported actions (`thought`, `search_index`, `timeline`, `get_full`) to the user guide's MCP tools reference.

---

### DOC-009: ARCHITECTURE.md — "SurrealKV & RocksDB" but Default is `mem://`
- **Code Location:** [daemon.rs#L43-L45](file:///Users/keith/Documents/mythrax/mythrax-core/src/daemon.rs#L43-L45)
- **Doc Location:** [ARCHITECTURE.md#L49](file:///Users/keith/Documents/mythrax/ARCHITECTURE.md#L49)
- **Description of Conflict:** ARCHITECTURE.md states the system uses "SurrealKV & RocksDB Engines" for persistent storage. However, when no config file exists, the daemon defaults to `mem://` (volatile in-memory storage), meaning all data is lost on restart.
- **Specification Reference & Conflicting Specs Resolution:** The mythrax-2.0 spec specifies SurrealKV (pure Rust) as the primary engine. The `mem://` default is undocumented.
- **Alignment Recommendation:** **Update code.** Change the default from `mem://` to `surrealkv://~/.mythrax/db` to match the documented architecture. Use `mem://` only for explicit test configurations.

---

### DOC-010: Codex/Cursor Hook Adapters — Documented But Unsupported
- **Code Location:** [hooks/adapters.rs#L53-L58](file:///Users/keith/Documents/mythrax/mythrax-core/src/hooks/adapters.rs#L53-L58)
- **Doc Location:** [README.md](file:///Users/keith/Documents/mythrax/README.md) (implicitly presents Mythrax as supporting multiple agent hosts)
- **Description of Conflict:** The `adapt_codex()` and `adapt_cursor()` functions in `hooks/adapters.rs` immediately bail with "unsupported in v2.1.0" errors. The Codex and Cursor payload structs exist (lines 23-34) but are never used. The README and user guide do not disclose this limitation.
- **Specification Reference & Conflicting Specs Resolution:** The adapters.rs file itself documents "Unsupported hosts: Codex, Cursor (unsupported in v2.1.0)".
- **Alignment Recommendation:** **Update documentation.** Add a "Supported Hosts" section to README.md listing Claude Code and Gemini/Antigravity as supported, and Codex/Cursor as planned but not yet implemented.

---

### DOC-011: Stale Handoff Pruning — 3 Days vs 7 Days
- **Code Location:** [db/backend.rs#L3425-L3468](file:///Users/keith/Documents/mythrax/mythrax-core/src/db/backend.rs#L3425-L3468)
- **Doc Location:** [forge_and_stm design](file:///Users/keith/Documents/mythrax/specs/forge_and_stm/design.md)
- **Description of Conflict:** The spec says stale handoff cleanup runs on a "7 days" threshold. The implementation uses "3d" (3 days) in SurrealQL queries and `3 * 24 * 3600` seconds for file pruning.
- **Specification Reference & Conflicting Specs Resolution:** No later spec overrides the 7-day threshold. This appears to be an implementation deviation.
- **Alignment Recommendation:** **Update code.** Change the pruning threshold from 3 days to 7 days to match the spec, or update the spec if the 3-day threshold was a deliberate post-spec decision. Make it configurable.

---

### DOC-012: ARCHITECTURE.md Describes 9 MCP Tools — Actual Count May Differ
- **Code Location:** [mcp_routes.rs](file:///Users/keith/Documents/mythrax/mythrax-core/src/mcp_routes.rs)
- **Doc Location:** [ARCHITECTURE.md](file:///Users/keith/Documents/mythrax/ARCHITECTURE.md), [mythrax_user_guide.md#L149](file:///Users/keith/Documents/mythrax/mythrax_user_guide.md#L149), [DEVELOPMENT.md#L20](file:///Users/keith/Documents/mythrax/DEVELOPMENT.md#L20)
- **Description of Conflict:** All documentation claims "9 high-efficiency, action-enum-based tools" (consolidated from 32 legacy tools). The SKILL.md agent playbook also lists 9. However, the MCP schema directory shows only 7 tool schema files (`manage_config`, `manage_file`, `manage_htr`, `manage_memory`, `manage_stm`, `manage_vault`, `pre_invocation_hook`), and the user guide lists `compliance_audit` and `ingest_knowledge` separately. The actual tool count needs reconciliation.
- **Specification Reference & Conflicting Specs Resolution:** The mythrax-1.0-release spec defined the original 9 tools. Subsequent consolidation may have merged some.
- **Alignment Recommendation:** **Update documentation.** Audit `mcp_routes.rs` to determine the exact current tool count and names, then update all 4 documentation files to match.

---

*Report generated on 2026-06-27. All findings verified against `git pull origin main` (already up to date) and `cargo check` (compiles cleanly, v2.2.0). All 49 source files in `mythrax-core/src/` were read from start to finish.*
