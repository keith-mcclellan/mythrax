## Title: Missing test coverage for public functions in cognitive and pipeline modules


**Labels:** `bug`, `agent-found`

### Description
An analysis of test coverage reveals that multiple public functions in `mythrax-core` have no corresponding tests in the eval or test suite. This violates the testing requirements and risks unnoticed regressions.

Specific untested functions:
- `mythrax-core/src/cognitive/pipeline/signals.rs:41` - `cluster_facts`
- `mythrax-core/src/cognitive/pipeline/signals.rs:118` - `get_active_stm_anchors`
- `mythrax-core/src/cognitive/ast.rs:15` - `extract_code_ast`
- `mythrax-core/src/cognitive/ast.rs:210` - `save_ast_symbols_to_vault`
- `mythrax-core/src/verify.rs:12` - `run_workspace_audit`

### Minimal Reproducible Scenario
Run `cargo test --no-default-features -- cognitive::pipeline::signals` and observe no tests covering `cluster_facts`.

### Severity
Medium - Lack of coverage increases the risk of subtle logic bugs going unnoticed.

### Suggested Fix
Add comprehensive unit and integration tests for the listed functions to cover their behavior, edge cases, and panic paths.
