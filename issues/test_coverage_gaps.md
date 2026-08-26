---
labels: bug, agent-found
severity: Medium
---

# Test coverage gaps in mythrax-core public functions

## Description
The following public functions in `mythrax-core` have no corresponding tests in the eval or test suite. This gap in test coverage risks logic regressions going unnoticed.

## Untested Functions

- `mythrax-core/src/config.rs` line 8: `daemon_port`
- `mythrax-core/src/config.rs` line 15: `daemon_url`
- `mythrax-core/src/config.rs` line 19: `llm_proxy_url`
- `mythrax-core/src/daemon.rs` line 44: `update_last_activity`
- `mythrax-core/src/daemon.rs` line 818: `backup_vault_folders`
- `mythrax-core/src/daemon.rs` line 872: `check_disk_space`
- `mythrax-core/src/daemon.rs` line 926: `check_swap_pressure`
- `mythrax-core/src/mcp_routes.rs` line 25: `strip_nulls`
- `mythrax-core/src/mcp_routes.rs` line 49: `strip_diffs`
- `mythrax-core/src/mcp_routes.rs` line 95: `truncate_summary`
- `mythrax-core/src/mcp_routes.rs` line 157: `get_mcp_tools_schema`

## Minimal Reproducible Scenario
1. Run `grep -rnw "pub fn" mythrax-core/src/` to list all public functions.
2. Run `grep -r "fn test_" mythrax-core/src/ evals/` to list all test functions.
3. Compare the lists. The functions listed above do not appear in the test suite.

## Suggested Fix
Write unit tests for each of the identified public functions to ensure their behavior is verified by the test suite.
