---
title: "Panic Path in MetaSkill Synthesizer on Malformed LLM JSON"
labels: ["bug", "agent-found"]
---

## Description
There is a high-severity logic bug in the local agent `meta_skill.rs` file where a malformed JSON response from the LLM could cause a panic, entirely crashing the agent node.

## Location
`mythrax-core/src/cognitive/meta_skill.rs`, lines 309-316

## Minimal Reproducible Scenario
When the `local_code_writer` agent (or meta synthesizer) triggers the `merge_skills` logic and the LLM responds with a JSON object that omits `source_skills`, `suggested_target_name`, `similarity`, or `reason` (or provides them in incorrect types), the code attempts to `.unwrap()` the JSON structure:

```rust
let sources = sug["source_skills"].as_array().unwrap();
let src_names: Vec<String> = sources.iter().map(|s| s.as_str().unwrap().to_string()).collect();
```
If `sug["source_skills"]` is `Null` or a string rather than an array, `as_array()` returns `None` and the unwrap panics, killing the process and breaking the background compaction/skills synthesis loop.

## Severity
High

## Suggested Fix
Replace the `.unwrap()` calls with safe bindings and fallback gracefully, logging an error and skipping the invalid suggestion:

```rust
if let (Some(sources_arr), Some(target_name), Some(sim), Some(reason)) = (
    sug["source_skills"].as_array(),
    sug["suggested_target_name"].as_str(),
    sug["similarity"].as_f64(),
    sug["reason"].as_str()
) {
    let src_names: Vec<String> = sources_arr.iter().filter_map(|s| s.as_str().map(|v| v.to_string())).collect();
    // process safely
} else {
    log::warn!("Received malformed skill merge suggestion from LLM: {}", sug);
}
```
