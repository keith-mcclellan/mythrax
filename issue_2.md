# Panic Path: Unsafe `.unwrap()` on LLM JSON array extraction

**Labels:** `bug`, `agent-found`
**Severity:** High

## Vulnerability Details
**File:** `mythrax-core/src/cognitive/meta_skill.rs`
**Line:** 309

In `detect_skill_merges`, the LLM output is parsed as JSON, and the `source_skills` field is accessed using `.as_array().unwrap()`. If the LLM generates a response missing this key, or provides a non-array value, the unwrap will panic, crashing the daemon. The same applies for the `.as_str().unwrap()` when mapping elements.

**Code Snippet:**
```rust
let sources = sug["source_skills"].as_array().unwrap();
let src_names: Vec<String> = sources.iter().map(|s| s.as_str().unwrap().to_string()).collect();
```

## Minimal Reproducible Scenario
1. Create two overlapping AI Agent Playbooks (SKILL.md) in the vault to trigger the `detect_skill_merges` routine.
2. Intercept the LLM response (or mock the model broker) for the skill merge validator prompt.
3. Return a JSON response missing the `source_skills` key, e.g.: `{ "should_merge": true, "suggested_name": "meta-test", "reason": "overlap" }`.
4. The function attempts to parse the JSON and calls `.unwrap()` on the missing `source_skills` field, resulting in a daemon panic.

## Suggested Fix
Use safe unwrapping, e.g., `if let Some(sources) = sug["source_skills"].as_array()` and `.as_str().unwrap_or_default()`.
