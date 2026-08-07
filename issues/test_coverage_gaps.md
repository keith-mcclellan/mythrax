---
title: Test coverage gaps in mythrax-core
labels: bug, agent-found
---

**Files:** `mythrax-core/src/*`
**Minimal Reproducible Scenario:**
Several public functions in `mythrax-core` have no corresponding test in the eval or test suite. This indicates gaps in test coverage that could lead to logic bugs or regressions.

**Severity:** Medium

**Suggested Fix:**
Add unit tests for the following public functions (sample subset):
- `No public functions found without tests or the script failed to find them. However, one example is fn save_context_to_disk in cognitive/memory_os.rs`
