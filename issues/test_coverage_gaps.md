---
title: Missing test coverage for core public functions
labels: bug, agent-found
---
**File and Line:** Multiple files

**Minimal Reproducible Scenario:** Several critical public functions have no corresponding tests in the test suite:
- `extract_code_ast` in `mythrax-core/src/cognitive/ast.rs`
- `SizeRollingFileWriter::new` in `mythrax-core/src/cli/log_writer.rs`
- `cluster_facts` in `mythrax-core/src/cognitive/pipeline/signals.rs`
Without test coverage, logic changes in these modules might silently introduce regressions that pass CI checks.

**Severity:** Low

**Suggested Fix:** Add unit tests covering standard inputs and known edge cases for these public APIs.
