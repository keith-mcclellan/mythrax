---
title: Missing Test Coverage for Public Functions
labels: bug,agent-found
---

**Files:** Various

**Description:**
The following public functions have no tests in the eval or test suite:
- `truncate_summary` in `mythrax-core/src/mcp_routes.rs`
- `derive_slug` in `mythrax-core/src/cognitive/pipeline/signals.rs`
- `resolve_rule_path` in `mythrax-core/src/cognitive/pipeline/signals.rs`
- `slugify_title` in `mythrax-core/src/cognitive/pipeline/signals.rs`
- `cluster_facts` in `mythrax-core/src/cognitive/pipeline/signals.rs`
- `get_active_stm_anchors` in `mythrax-core/src/cognitive/pipeline/signals.rs`
- `extract_symbols` in `mythrax-core/src/cognitive/paging.rs`

**Severity:** Medium

**Minimal Reproducible Scenario:**
N/A - Coverage gap analysis.

**Suggested Fix:**
Add unit tests for these public functions, particularly `extract_symbols` due to its critical role.