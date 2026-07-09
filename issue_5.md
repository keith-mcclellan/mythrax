---
labels: bug, agent-found
---

# Logic Bug: `search_filtered` silently fails to apply filter if zero matches found

## Location
- File: `mythrax-core/src/db/backend.rs`
- Function: `search_filtered`
- Lines: 1802-1806

## Description
The `search_filtered` function attempts to filter results returned from `self.search` by checking if the episode matches the provided `concepts` and `files`. Matches are pushed into `filtered_results`. However, if `filtered_results` is empty (meaning no documents matched the filter criteria), the code executes a fallback:
```rust
let final_results = if filtered_results.is_empty() {
    unfiltered.results
} else {
    filtered_results
};
```
This means if the user explicitly applies a strict filter that should yield 0 results, the system will instead return the full unfiltered response (up to the limit). This silent failure is a correctness issue and breaks the API contract of a "filtered search", producing false positives which are often worse than returning an empty set.

## Minimal Reproducible Scenario
1. Run `search_filtered` with a query and a non-existent file requirement (e.g. `files: ["does_not_exist.txt"]`).
2. The internal logic finds matches for the query, but none contain the required file, leaving `filtered_results` empty.
3. The method returns `unfiltered.results` instead of an empty array.

## Severity
High. Agents relying on strict filtering will receive unrelated results, leading to hallucinations and incorrect context.

## Suggested Fix
Remove the fallback behavior and unconditionally return the filtered list:
```rust
let final_results = filtered_results;
```