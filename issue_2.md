---
title: "Bug: Panic in `meta_skill.rs` due to unsafe JSON unwrapping of LLM responses"
labels: ["bug", "agent-found"]
severity: "High"
---

## Description
The `detect_skill_merges` function parses a JSON response directly from the LLM. It accesses fields like `source_skills`, `suggested_target_name`, and `similarity` using `.as_array().unwrap()`, `.as_str().unwrap()`, and `.as_f64().unwrap()`. LLM outputs are inherently probabilistic and can occasionally omit fields or use unexpected types (e.g., returning an integer `1` instead of a float `1.0` for similarity, or returning a string instead of an array).

## File and Line Numbers
- `mythrax-core/src/cognitive/meta_skill.rs:309`: `let sources = sug["source_skills"].as_array().unwrap();`
- `mythrax-core/src/cognitive/meta_skill.rs:313`: `sug["suggested_target_name"].as_str().unwrap(),`

## Minimal Reproducible Scenario
1. Run `detect_skill_merges`.
2. Mock the LLM to return JSON that is missing the `source_skills` array, or where `similarity` is formatted as a string `"0.8"`.
3. The `.unwrap()` on the JSON value conversion will trigger a panic.

## Blast Radius
The meta-skill synthesis process will crash, preventing the system from automatically optimizing or deduplicating learned skills.

## Suggested Fix
Use safe accessors and provide fallback values or skip the malformed suggestion entirely.
```rust
let sources = sug["source_skills"].as_array().unwrap_or(&Vec::new());
let src_names: Vec<String> = sources.iter().filter_map(|s| s.as_str().map(|s| s.to_string())).collect();
let similarity = sug["similarity"].as_f64().unwrap_or(0.0);
```