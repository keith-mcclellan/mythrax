# Tasks: Dynamic Model Broker Integration & Data Flow Verification

## Phase 1: Code Integration & Testing

### [x] T1: Implement completions intercept in `completion_explicit`
* **Purpose**: Map model names to `ModelTier` and call `DYNAMIC_MODEL_BROKER.get().acquire_llm(tier).await` under `#[cfg(feature = "mlx")]` inside `LLMClient::completion_explicit` in [src/llm/mod.rs](file:///Users/keith/Documents/mythrax/mythrax-core/src/llm/mod.rs).
* **Validation**: Run `cargo check --features mlx` to verify clean compilation.

### [x] T2: Create completions integration test
* **Purpose**: Write `tests/test_completion_dynamic_server.rs` using `mlx-community/Qwen2.5-1.5B-Instruct-MLX-4bit` to verify that dynamic server spawning and completions run end-to-end natively.
* **Validation**: Run `cargo test --test test_completion_dynamic_server --features mlx`.

---

## Phase 2: LLM-Dependent Data Flow Verification

For each flow, verify the expected behavior under local MLX execution, compare outputs to assertions, document any divergences in `verification_review.md`, and ensure `ARCHITECTURE.md` is updated.

### [x] T3: Initialize Phase 2 Logging Tracker
* **Purpose**: Add Flows 6–9 and their test assertions to the Loop Verification State Tracker in `verification_review.md`.

### [x] T4: Verify Flow 6 (Ingestion & Extraction - Forge)
* **Actions**:
  - Run `cargo nextest run --test test_forge --features mlx`.
  - Verify that local MLX inference correctly splits and extracts rules/concepts from PDFs and files.
  - Update `verification_review.md` and check `ARCHITECTURE.md`.

### [x] T5: Verify Flow 7 (Session/Scope Compaction - Compactor)
* **Actions**:
  - Run `cargo nextest run --test test_abandoned_session_sweep --features mlx`.
  - Verify that local MLX inference correctly generates summaries and compacts git diffs/transcripts.
  - Update `verification_review.md` and check `ARCHITECTURE.md`.

### [x] T6: Verify Flow 8 (Continuous Compaction & dreaming - Dreaming)
* **Actions**:
  - Run `cargo nextest run --test test_arbor_htr_loop_lifecycle --features mlx`.
  - Verify that local MLX inference correctly clusters episodes and synthesizes permanent wiki nodes.
  - Update `verification_review.md` and check `ARCHITECTURE.md`.

### [x] T7: Verify Flow 9 (Model Broker & VRAM Eviction - Broker)
* **Actions**:
  - Run `cargo nextest run --test test_model_broker --features mlx`.
  - Verify that model tier switching evicts previous layers and clears Metal execution cache.
  - Update `verification_review.md` and check `ARCHITECTURE.md`.

### [x] T8: Run Full Integration Suite & Final Audit
* **Actions**:
  - Run `cargo nextest run --features mlx` to verify all 164+ tests pass cleanly.
  - Finalize all task statuses in `verification_review.md`.

