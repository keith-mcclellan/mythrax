---
title: "Bug: Panic when parsing LLM-generated JSON in detect_skill_merges"
labels: ["bug", "agent-found"]
---

## Description
In `mythrax-core/src/cognitive/meta_skill.rs`, around line 309, there are several `.unwrap()` calls on values extracted from the `sug` JSON object:

```rust
let sources = sug["source_skills"].as_array().unwrap();
let src_names: Vec<String> = sources.iter().map(|s| s.as_str().unwrap().to_string()).collect();
```

## Reproducible Scenario
1. The AI Agent pipeline triggers the `detect_skill_merges` function which prompts the LLM to review playbooks.
2. The LLM generates a JSON response that is successfully parsed by `serde_json::from_str` as valid JSON, but it might miss the expected `source_skills` array field, or return a scalar instead of an array.
3. The code calls `.as_array().unwrap()` which panics when the field is missing or not an array.

## Severity
**High** - AI models are non-deterministic and can easily generate slightly malformed schema outputs.

## Suggested Fix
Gracefully handle the missing or incorrect fields using `if let` or `and_then`:

```rust
if let Some(sources) = sug.get("source_skills").and_then(|s| s.as_array()) {
    let src_names: Vec<String> = sources.iter()
        .filter_map(|s| s.as_str().map(|s| s.to_string()))
        .collect();
    // ...
}
```
