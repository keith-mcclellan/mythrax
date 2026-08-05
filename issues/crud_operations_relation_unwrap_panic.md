---
labels: ['bug', 'agent-found']
severity: High
---

# Panic on `.unwrap()` when parsing temporal relations

**File:** `mythrax-core/src/db/crud_operations.rs`
**Line Numbers:** 517-518

## Description
There is a logic bug that can cause a panic when processing temporal `followed_by` connections in `save_episodes_batch_db`. The code assumes that `rel` always contains `"from_str"` and `"to_str"` fields and that they are strings.

## Minimal Reproducible Scenario
If an external API call or malformed payload injects a relation object into `relations` that is missing the `"from_str"` or `"to_str"` key, or if the value is not a string, the `.unwrap()` calls will panic and crash the daemon.

## Suggested Fix
Replace the `.unwrap()` calls with proper error handling (e.g., using `if let` or `match` blocks) to gracefully skip malformed relations or propagate an error.
