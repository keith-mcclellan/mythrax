---
labels: bug, agent-found
---

# Panic Path: Unsafe JSON unwrap in `meta_skill.rs`

## Location
- File: `mythrax-core/src/cognitive/meta_skill.rs`
- Lines: 309, 310, 313, 315, 316

## Description
In `scan_and_merge_skills`, the code processes an LLM response parsed as JSON (`suggestions`). It assumes the JSON structure is perfectly matching the prompt schema and uses hard `unwrap()` calls on options returned by `as_array()`, `as_str()`, and `as_f64()`.
```rust
let sources = sug["source_skills"].as_array().unwrap();
let src_names: Vec<String> = sources.iter().map(|s| s.as_str().unwrap().to_string()).collect();
```
If the LLM hallucinates a slightly different key (e.g. `source_skill`) or returns a string instead of an array, this causes an unrecoverable panic, crashing the agent runtime.

## Minimal Reproducible Scenario
1. Trigger `scan_and_merge_skills` in a scope where there are multiple skills.
2. Mock the LLM to return valid JSON but with an incorrect type, e.g. `[{"source_skills": "just one skill", "suggested_target_name": "...", "similarity": 0.9, "reason": "..."}]`.
3. The method panics at `.as_array().unwrap()`.

## Severity
High. Unhandled panics from processing non-deterministic external outputs (LLM generations) are a major stability risk.

## Suggested Fix
Use safe JSON parsing and fallback or skip invalid items instead of unwrapping:
```rust
let sources = match sug.get("source_skills").and_then(|s| s.as_array()) {
    Some(arr) => arr,
    None => continue,
};
let src_names: Vec<String> = sources.iter().filter_map(|s| s.as_str().map(|s| s.to_string())).collect();
// Similar for other fields using `.and_then` or `if let`.
```