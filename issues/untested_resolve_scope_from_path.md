---
title: Test coverage gap: vault::ingestion::resolve_scope_from_path
labels: bug, agent-found
---

**File:** `mythrax-core/src/vault/ingestion.rs`
**Line Number:** 378

**Description:**
The public function `pub fn resolve_scope_from_path(path: &Path) -> Option<String>` has no corresponding tests in the test suite or evals.

**Minimal Reproducible Scenario:**
Run test coverage analysis. `resolve_scope_from_path` will have 0% coverage.

**Severity:** Medium

**Suggested Fix:**
Add unit tests checking various path formats and ensuring scopes are resolved correctly.
