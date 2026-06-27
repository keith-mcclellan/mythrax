# Loop Design: Phased LLM Broker Implementation and Verification

This document specifies the execution loop design for implementing the production dynamic model broker completions integration and verifying the LLM-dependent data flows.

## 1. Objective and Mode
* **Objective**: Execute and verify the integration of the dynamic model broker into LLM completions and perform sequential data flow verification of LLM-dependent flows (Flows 6–9), logging ground truth and divergences in `verification_review.md`.
* **Inputs Watched**: `tasks.md` and the Rust codebase.
* **Outputs Allowed**: Rust code changes in `src/llm/mod.rs`, new test file `tests/test_completion_dynamic_server.rs`, documentation updates in `ARCHITECTURE.md` and `verification_review.md`.
* **Loop Mode**: Ephemeral (human-initiated run-to-completion).

---

## 2. Loop Design
The loop uses a state-machine execution flow:
1. **Discovery**: Read `tasks.md` and find the first item that is not completed `[ ]` or is in progress `[/]`.
2. **Handoff**: Assign the task to the executor.
3. **Execution**:
   * **Phase 1 (Implementation)**: Integrate the dynamic model broker completions intercept in `src/llm/mod.rs` and implement the integration test.
   * **Phase 2 (Verification)**: Verify Flows 6, 7, 8, 9 using the test suite under the `mlx` feature, logging observed behaviors and documenting divergences.
4. **Verification**: Invoke the evaluator to test the step's changes.
5. **Persistence**: Update `tasks.md` and write findings to `verification_review.md`.
6. **Scheduling**: Transition to the next task if verified, or return control to the human if blocked/completed.

---

## 3. Evaluator
* **Role**: Adversarial reviewer. Assumes output is broken until proven otherwise.
* **Checks**:
  * For Phase 1: `cargo check --features mlx` and `cargo test --test test_completion_dynamic_server --features mlx` must pass cleanly without warnings.
  * For Phase 2: Run specific cargo test commands mapped to Flows 6, 7, 8, and 9.
* **Verdict Gates**:
  * **PASS**: Compilation is clean, test outputs match expected behaviors, and `verification_review.md` is updated.
  * **REJECT**: Compilation fails, tests fail, warnings are present, or a data flow discrepancy is undocumented.

---

## 4. State
The loop maintains two local state files:
1. **`tasks.md`**: Tracks progress checklist (`[ ]`, `[/]`, `[x]`).
2. **`verification_review.md`**: Stores execution data, command outputs, observed behaviors, and any divergences.

---

## 5. Trigger / Invocation
* **Trigger**: Initial user approval to execute Phase 1 of the implementation plan.
* **Re-invocation**: Re-entry via subsequent turns. The loop checks the state files to resume cleanly from the last uncompleted task.

---

## 6. Limits
* **Max Retries per Task**: 2 attempts to fix compilation/test failures before returning control.
* **Max Parallel Subagents**: 1 local subagent executor.
* **Execution Timeout**: 60 seconds per shell command execution.
* **Blast Radius**: Low. Code changes are local, reversible via `git checkout`, and verified in isolated test beds.

---

## 7. Human Boundary / Return of Control
The loop will stop and return control to the user:
* **Immediate Block**: If a test or build fails after 2 retry attempts.
* **Phase Seam**: At the end of Phase 1 (before starting data flow verification).
* **Divergence Checkpoint**: During Phase 2, if a divergence is discovered between the code implementation and expected documentation assertions.
* **Completion**: When all tasks are marked `[x]` and the audit yields a `PASS`.

---

## 8. First Implementation Sketch
1. Read `tasks.md`.
2. Locate task `T1: Integrate completions routing to DynamicModelBroker`.
3. Set status to `[/]` in `tasks.md`.
4. Implement the completion intercept in `src/llm/mod.rs`.
5. Verify via `cargo check --features mlx`.
6. Proceed to test creation.
