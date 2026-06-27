# Loop Design: Phased Implementation and Verification

This document specifies the execution loop design for implementing code fixes and verifying system data flows.

## 1. Objective and Mode
* **Objective**: Execute the tasks in [tasks.md](file:///Users/keith/Documents/mythrax/specs/docs_and_sweep/tasks.md) sequentially, validating each phase with an independent evaluator check and logging ground truth/divergences in [verification_review.md](file:///Users/keith/Documents/mythrax/specs/docs_and_sweep/verification_review.md).
* **Inputs Watched**: `tasks.md` and the Rust codebase.
* **Outputs Allowed**: Rust code changes in `synthesis.rs`, `main.rs`, `test_abandoned_session_sweep.rs`, documentation updates in `ARCHITECTURE.md` and `DEVELOPMENT.md`, and review logging in `verification_review.md`.
* **Loop Mode**: Ephemeral (human-initiated run-to-completion).

---

## 2. Loop Design
The loop uses a state-machine execution flow:
1. **Discovery**: Read `tasks.md` and find the first item that is not completed `[ ]` or is in progress `[/]`.
2. **Handoff**: Assign the task to the execution subagent (`local_code_writer` for Phase 1 coding; cloud agent for Phase 2 docs).
3. **Execution**:
   * **Phase 1**: Write code, compilation fixes, and integration tests.
   * **Phase 2**: Write doc flow mappings, execute tests/CLI checks, and log results.
4. **Verification**: Invoke the evaluator to test the step's changes.
5. **Persistence**: Update `tasks.md` and write findings/divergences to `verification_review.md`.
6. **Scheduling**: Transition to the next task if verified, or return control to the human if blocked/completed.

---

## 3. Evaluator
* **Role**: Adversarial reviewer. Assumes output is broken until proven otherwise.
* **Checks**:
  * For Phase 1: `cargo check` and `cargo test --test test_abandoned_session_sweep` must compile and pass cleanly without warnings.
  * For Phase 2: Run specific cargo test commands mapped to each flow. Compare command outputs to documented assertions.
* **Verdict Gates**:
  * **PASS**: Compilation is clean, test outputs match documented assertions with zero failures, and `verification_review.md` is updated.
  * **REJECT**: Compilation fails, tests fail, warnings are present, or a documentation discrepancy is detected but undocumented.

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
* **Phase Seam**: At the end of Phase 1 (before writing documentation) to verify code updates.
* **Divergence Checkpoint**: During Phase 2, if a divergence is discovered between the code implementation and expected documentation assertions.
* **Completion**: When all tasks are marked `[x]` and the audit yields a `PASS`.

---

## 8. First Implementation Sketch
1. Read `tasks.md`.
2. Locate task `T1: Implement Background Sweep`.
3. Set status to `[/]` in `tasks.md`.
4. Delegate coding of T1 to `local_code_writer` using the local Qwen model.
5. Execute the code changes in the workspace.
6. Verify via `cargo check`.
7. Move to T2.
