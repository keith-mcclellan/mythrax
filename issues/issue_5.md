---
title: Panic vulnerability in API routing via unwrap() inside tests (Architecture Violation)
labels: bug, agent-found
---

**File:** `mythrax-core/src/api.rs`
**Line:** 881, 882, 884, 885, etc. (36 instances)

**Severity:** Low (Test paths, but strict architecture violation)

**Issue:**
There are over 30 instances of `.unwrap()` calls in `mythrax-core/src/api.rs`, primarily within the test block `test_api_routes`. While these are within test bounds, the architecture guidelines (`AGENTS.md`) strictly mandate: "Panic & Stub Elimination: Production paths must contain zero `.unwrap()`, `.expect()`, `todo!()`, `unimplemented!()`, or `// TODO` stubs." If any of this test code is ever promoted or used as an example for production logic, it will violate this rule and crash on failures.

**Minimal Reproducible Scenario:**
Run the API tests in an environment where the in-memory database fails to initialize or tempfile fails. The test thread will panic.

**Suggested Fix:**
Refactor test functions to return `anyhow::Result<()>` and use the `?` operator for error propagation instead of `.unwrap()`.
