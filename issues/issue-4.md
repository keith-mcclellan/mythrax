---
title: "Bug: Test coverage gaps in core MCP and API functionalities"
labels: ["bug", "agent-found"]
---

**File:** `mythrax-core/src/mcp.rs`, `mythrax-core/src/api.rs`, `mythrax-core/src/verify.rs`
**Line:** N/A

**Minimal Reproducible Scenario:**
Several public functions in core modules completely lack unit or integration tests, including:
- `mcp.rs`: `new_local`, `get_current_token`, `ensure_daemon_active`, `call_tool`, `handle_request`, `get_htr_tree_json`
- `api.rs`: `has_checked_memory`, `mark_memory_checked`, `create_router`, `health_handler`, `check_auth`, etc.
- `verify.rs`: `run_workspace_audit`, `check_search_history`, `verify_daemon_health`, etc.

**Severity:** Low (Code might be functional but unverified during CI/eval).

**Suggested Fix:**
Add comprehensive unit tests for these public functions in the respective modules or in `evals/`.
