# Specification: Rust-Native Release-Only E2E SWE-bench Harness

This specification governs the native Rust re-implementation of the external Python-based SWE-bench evaluation harness and its integration into a gated, release-only end-to-end integration test suite.

---

## Phase 1: Clarify

### Restated Request
The user wants to eliminate the external Python wrappers and scripts used for SWE-bench Verified evaluations (`run-batch`, `eval.sh`, and `summarize.py` in `evals/swebench/`). These components will be re-implemented natively in Rust within the `mythrax-core` crate and exposed as a specialized end-to-end (E2E) integration test suite. This suite will be ignored during normal development loops and only executed as a release gate.

### Known Facts
- **Existing Python implementation**:
  - `run-batch`: Batch generates LLM predictions and outputs `predictions.jsonl`.
  - `eval.sh`: Invokes `python -m swebench.harness.run_evaluation` to run the official Princeton Docker/Python evaluation scorer.
  - `summarize.py`: Tallies results (resolved, unresolved, error), calculates rates, computes percentage-point deltas, tracks per-instance status changes, and renders a markdown report.
  - `smoke-test.sh`: Runs a 1-instance mock test to validate the pipeline.
- **Git status**: Large dataset files are now ignored via `.gitignore`, keeping the repo clean.
- **Cargo test environment**: Integration tests are placed in `mythrax-core/tests/` and compiled/run via Cargo.

### Assumptions
1. **Mock Scorer Capability**: The Rust implementation must support a high-fidelity mock execution mode (mirroring the python `eval.sh --mock` capability) to allow local verification and pipeline checking without running heavy Docker containers.
2. **Official Scorer Integration**: For actual (non-mock) evaluations, the Rust code will execute the official Princeton python harness commands (`python -m swebench.harness.run_evaluation`) via `std::process::Command` at arm's length, preserving the official scoring integrity.
3. **Release Gate Triggers**: The release-only suite will be gated behind a custom Cargo feature `"release-e2e"` and marked `#[ignore]` by default.

### Tradeoffs
- **Complexity of python-to-rust port**: Porting the parsing, tallying, delta math, and markdown reporting to Rust increases Rust codebase size but guarantees compile-time safety and eliminates runtime python dependencies for analytics.
- **Execution Overhead**: Gating the suite behind `"release-e2e"` and `#[ignore]` completely avoids slowing down everyday local test execution.

---

## Phase 2: Requirements

### Problem
The existing SWE-bench harness relies on multiple loosely coupled Bash and Python scripts. This creates a maintenance burden, lacks compile-time safety, and prevents the evaluation suite from being easily run, maintained, or checked under the standard Rust toolchain.

### Outcome
A single, cohesive, compile-safe, and native Rust evaluation system integrated directly into the `cargo test` framework, capable of running predictions, executing evaluations (real or mock), and performing A/B comparison reporting.

### In Scope
- Porting prediction batching logic, mock prediction generation, and predictions output (`predictions.jsonl` writer) to Rust.
- Porting results parsing, statistical aggregation, delta calculations (percentage points), and markdown comparison report generation to Rust.
- Porting the official scorer wrapper command execution to Rust.
- Implementing a release-only integration test `tests/test_release_swebench_e2e.rs` gated by the Cargo feature `"release-e2e"` and ignored by default.

### Out of Scope
- Re-implementing the actual Princeton unit-test execution logic or Docker runner itself. (We still invoke their official Python harness at arm's length for real evaluations).

### Inputs
- Pinned dataset identifier (e.g. `princeton-nlp/SWE-bench_Verified`).
- Input prediction files or generated prediction structures.
- Active LLM client configurations (when running real predictions).
- Evaluation result files (when running comparison analysis).

### Outputs
- `predictions.jsonl`: Storing instance IDs and generated git patches.
- Output JSONL results (baseline vs. Mythrax).
- Markdown comparison report containing metrics, deltas, and status-change tables.

### Acceptance Criteria
- [ ] Bumping the Cargo version to `2.1.0` and declaring a `"release-e2e"` feature flag.
- [ ] Standard `cargo test` run excludes the new E2E test by default.
- [ ] Running `cargo test --test test_release_swebench_e2e --features release-e2e -- --ignored` executes the full mock evaluation pipeline in Rust.
- [ ] The Rust comparison engine calculates baseline resolve rate (30.00%) and Mythrax resolve rate (35.60%) on the mock 500-instance set, asserting a delta of exactly `+5.60 percentage points` and `+28` resolved instances.
- [ ] The comparison engine outputs a formatted Markdown report matching the schema of the Python-based report.

---

## Phase 3: Design

### Overview
We will implement the SWE-bench harness natively within a new `swebench` module inside `mythrax-core/src/bench/`.

### Execution Flow
1. **Runner Bootstrapping**: The integration test instantiates `SweBenchRunner` with a `SweBenchConfig`.
2. **Prediction Generation**:
   - In mock mode: Writes a hardcoded single-instance prediction to `predictions.jsonl`.
   - In real mode: Iterates over the dataset, queries the LLM, and writes patches.
3. **Evaluation Execution**:
   - In mock mode: Writes realistic 500-instance baseline (`mock_baseline.jsonl`) and Mythrax (`mock_mythrax.jsonl`) files.
   - In real mode: Runs `std::process::Command` to invoke the Princeton python runner.
4. **Comparative Analysis**:
   - Parses the two result JSONL files.
   - Tallies status counts, computes percentages, calculates rate differences, identifies status changes, and generates the markdown report.

### Interfaces

#### Crate Features (Cargo.toml)
```toml
[features]
release-e2e = []
```

#### Structs and Types
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweInstancePrediction {
    pub instance_id: String,
    pub model_name_or_path: String,
    pub model_patch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweInstanceResult {
    pub instance_id: String,
    pub status: String, // "resolved" | "unresolved" | "error"
}

pub struct ComparisonReport {
    pub total_base: usize,
    pub total_current: usize,
    pub resolved_base: usize,
    pub resolved_current: usize,
    pub rate_base: f64,
    pub rate_current: f64,
    pub delta_resolved: i32,
    pub delta_rate: f64,
    pub markdown: String,
}
```

---

## Phase 4: Test Plan

### Unit & Integration Tests (tests/test_release_swebench_e2e.rs)
- `test_swebench_mock_pipeline_reproducibility`:
  - Run predictor in mock mode -> Verify `predictions.jsonl` schema.
  - Run scorer in mock mode -> Generate mock baseline and mythrax files.
  - Run analyzer -> Parse results, calculate A/B statistics, verify baseline is 30.00% and Mythrax is 35.60%, verify delta is exactly `+5.60 percentage points` (+28 resolved), and verify status transitions are detected correctly.
  - Print the generated Markdown report.

---

## Phase 5: Implementation Tasks

### T1: Cargo Manifest Configuration
- **Purpose**: Declare the new `"release-e2e"` feature flag.
- **Action**: Add `release-e2e = []` under the `[features]` section in `mythrax-core/Cargo.toml`.
- **Validation**: Run `cargo check --features release-e2e` to verify it compiles.

### T2: Rust-Native SWE-bench Harness Implementation
- **Purpose**: Implement predictor, scorer, and comparison analytics in Rust.
- **Files**:
  - `mythrax-core/src/bench/swebench/mod.rs`
  - `mythrax-core/src/bench/swebench/predictor.rs`
  - `mythrax-core/src/bench/swebench/scorer.rs`
  - `mythrax-core/src/bench/swebench/analyzer.rs`
- **Action**: Implement struct models, file parsers, math engines, and markdown formatters.
- **Validation**: Verify that modules compile cleanly.

### T3: Integration Test Suite
- **Purpose**: Create the release-only integration test.
- **File**: `mythrax-core/tests/test_release_swebench_e2e.rs`
- **Action**: Implement the E2E test, gating it with `#[cfg(feature = "release-e2e")]` and `#[ignore]`.
- **Validation**: Run `cargo test --test test_release_swebench_e2e --features release-e2e -- --ignored` and verify it passes.

---

## Phase 6: Validation
*(To be completed during implementation execution)*
- **Acceptance Criteria**: Ensure all automated tests pass.
- **Default Exclusion**: Ensure `cargo test` ignores the E2E test when no feature flag is provided.
