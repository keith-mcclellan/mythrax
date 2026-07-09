---
labels: bug, agent-found
---

# Out of Bounds Panic in `forge.rs`

## Location
- File: `mythrax-core/src/cognitive/forge.rs`
- Lines: 145, 153

## Description
When processing chunks and generating bidirectional links between them, the code accesses the `chunk_uuids` array via `chunk_uuids[idx - 1]` and `chunk_uuids[idx + 1]`. While there is a check `if idx + 1 < total_chunks`, `total_chunks` is calculated previously (e.g., from `chunks_data.len()`), but the length of `chunk_uuids` could theoretically diverge if chunk generation encountered issues, or the loop index otherwise exceeds the actual array bounds of `chunk_uuids`.

## Minimal Reproducible Scenario
1. Under edge conditions, `chunks_data.len()` might differ from the precomputed `chunk_uuids.len()`.
2. When the loop reaches an index where `idx + 1` evaluates to the length of `chunk_uuids` (but is less than `total_chunks`), indexing `chunk_uuids[idx + 1]` triggers a panic.

## Severity
Medium. Potential crash on ingestion pipeline for very specific chunk generation edge cases.

## Suggested Fix
Use the `.get()` method instead of direct indexing for array access to ensure safe bounds checking.
```rust
let prev_str = if idx > 0 {
    if let Some(uuid) = chunk_uuids.get(idx - 1) {
        let prev_path = format!("wiki/{}/chunk_{}_{}", normalized_scope, sanitized_source_name, uuid);
        format!("[[{}|Chunk {}]]", prev_path, idx)
    } else {
        "None".to_string()
    }
} else {
    "None".to_string()
};
```